//! Glue between the Slint UI and `groupkeep-core`.
//!
//! The UI never holds the source of truth: every edit calls back into Rust, Rust updates the
//! [`Group`], recomputes the [`Report`], and pushes fresh view rows. Lists are updated row-by-row
//! when their shape is unchanged (so text fields keep focus while typing) and fully reset when the
//! shape changes or a different expense is opened.

use crate::*;
use groupkeep_core::calc::{self, trim_num, Report};
use groupkeep_core::model::*;
use groupkeep_core::money::Currency;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel, Weak};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq)]
enum Pending {
    NewGroup,
    OpenFile,
    Quit,
    DeleteExpense,
}

#[derive(Default)]
struct State {
    group: Group,
    path: Option<PathBuf>,
    dirty: bool,
    editing: Option<Id>,
    ov_editing: Option<Id>,
    pending: Option<Pending>,
}

#[derive(Default)]
struct Models {
    people: Rc<VecModel<PersonRow>>,
    people_names: Rc<VecModel<SharedString>>,
    expenses: Rc<VecModel<ExpenseRow>>,
    overrides: Rc<VecModel<OverrideRow>>,
    summary: Rc<VecModel<SummaryRow>>,
    transfers: Rc<VecModel<TransferRow>>,
    rounding: Rc<VecModel<SharedString>>,
    warnings: Rc<VecModel<SharedString>>,
    contributions: Rc<VecModel<ContributionRow>>,
    ov_scopes: Rc<VecModel<SharedString>>,
    edit_methods: Rc<VecModel<MethodRow>>,
    edit_shares: Rc<VecModel<ShareRow>>,
    edit_items: Rc<VecModel<ItemRow>>,
    edit_days: Rc<VecModel<ChipRow>>,
    edit_overrides: Rc<VecModel<OverrideRow>>,
    edit_warnings: Rc<VecModel<SharedString>>,
    edit_rounding: Rc<VecModel<SharedString>>,
}

pub struct Controller {
    ui: Weak<AppWindow>,
    st: RefCell<State>,
    m: Models,
    toast_timer: slint::Timer,
}

fn ss(s: impl AsRef<str>) -> SharedString {
    SharedString::from(s.as_ref())
}

fn strings(v: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        v.into_iter().map(SharedString::from).collect::<Vec<_>>(),
    ))
}

/// Update a model in place when its length is unchanged (keeps text-field focus), else reset it.
fn sync<T: Clone + PartialEq + 'static>(m: &VecModel<T>, v: Vec<T>, force: bool) {
    if force || m.row_count() != v.len() {
        m.set_vec(v);
        return;
    }
    for (i, row) in v.into_iter().enumerate() {
        if m.row_data(i).as_ref() != Some(&row) {
            m.set_row_data(i, row);
        }
    }
}

fn parse_f64(s: &str) -> Option<f64> {
    let t: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    if t.is_empty() {
        None
    } else {
        t.parse().ok()
    }
}

fn method_key(m: Method) -> &'static str {
    match m {
        Method::ByItem => "item",
        Method::ByAttendance => "attendance",
        Method::Shares => "shares",
        Method::Percent => "percent",
        Method::Exact => "exact",
        Method::Equal => "equal",
    }
}

/// Methods offered for an expense: the type's list plus its current method if it came from another type.
fn methods_for(g: &Group, e: &Expense) -> Vec<Method> {
    let mut v = g.group_type.methods();
    if !v.contains(&e.method) {
        v.push(e.method);
    }
    v
}

impl Controller {
    pub fn setup(ui: &AppWindow) -> Rc<Controller> {
        let ctl = Rc::new(Controller {
            ui: ui.as_weak(),
            st: RefCell::new(State::default()),
            m: Models::default(),
            toast_timer: slint::Timer::default(),
        });

        // Static lists.
        ui.set_type_labels(strings(
            GroupType::ALL.iter().map(|t| t.label().to_string()),
        ));
        ui.set_type_blurbs(strings(
            GroupType::ALL.iter().map(|t| t.blurb().to_string()),
        ));
        ui.set_currency_labels(strings(Currency::ALL.iter().map(|c| c.label())));
        ui.set_ov_kind_labels(strings(OverrideKind::LABELS.iter().map(|s| s.to_string())));

        // Live models.
        let m = &ctl.m;
        ui.set_people(m.people.clone().into());
        ui.set_people_names(m.people_names.clone().into());
        ui.set_expenses(m.expenses.clone().into());
        ui.set_overrides(m.overrides.clone().into());
        ui.set_summary(m.summary.clone().into());
        ui.set_transfers(m.transfers.clone().into());
        ui.set_rounding_notes(m.rounding.clone().into());
        ui.set_warnings(m.warnings.clone().into());
        ui.set_contributions(m.contributions.clone().into());
        ui.set_ov_scope_labels(m.ov_scopes.clone().into());
        ui.set_edit_methods(m.edit_methods.clone().into());
        ui.set_edit_shares(m.edit_shares.clone().into());
        ui.set_edit_items(m.edit_items.clone().into());
        ui.set_edit_days(m.edit_days.clone().into());
        ui.set_edit_overrides(m.edit_overrides.clone().into());
        ui.set_edit_warnings(m.edit_warnings.clone().into());
        ui.set_edit_rounding(m.edit_rounding.clone().into());

        Self::wire(&ctl, ui);
        ctl.load_group_into_ui();
        ctl
    }

    pub fn ui(&self) -> AppWindow {
        self.ui.upgrade().expect("window alive")
    }

    /// Replace the whole group (used by tests/screenshots and file open).
    pub fn set_group(&self, g: Group, path: Option<PathBuf>) {
        {
            let mut st = self.st.borrow_mut();
            st.group = g;
            st.path = path;
            st.dirty = false;
            st.editing = None;
        }
        self.load_group_into_ui();
    }

    pub fn group(&self) -> Group {
        self.st.borrow().group.clone()
    }

    fn toast(&self, msg: impl Into<String>) {
        let ui = self.ui();
        ui.set_status(ss(msg.into()));
        let w = self.ui.clone();
        self.toast_timer.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(3500),
            move || {
                if let Some(ui) = w.upgrade() {
                    ui.set_status(SharedString::new());
                }
            },
        );
    }

    /// Push the two-way group fields (only on load/new, never while typing).
    fn load_group_into_ui(&self) {
        let ui = self.ui();
        {
            let st = self.st.borrow();
            let g = &st.group;
            ui.set_group_name(ss(&g.name));
            ui.set_group_description(ss(&g.description));
            ui.set_start_date(ss(&g.start_date));
            ui.set_num_days(g.num_days.max(1) as i32);
            ui.set_currency_index(g.currency.index() as i32);
            ui.set_editing_expense(false);
            ui.set_ov_open(false);
        }
        self.refresh(true, true);
    }

    // ------------------------------------------------------------------ refresh

    fn refresh(&self, force_lists: bool, force_edit: bool) {
        let ui = self.ui();
        let st = self.st.borrow();
        let g = &st.group;
        let cur = g.currency;
        let r: Report = calc::compute(g);
        let money = |v: i64| ss(cur.format(v));

        ui.set_file_name(ss(st
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()));
        ui.set_dirty(st.dirty);
        ui.set_type_index(g.group_type.index() as i32);
        ui.set_uses_days(g.group_type.uses_days());
        ui.set_uses_contributions(g.group_type.uses_contributions() || !g.contributions.is_empty());
        ui.set_member_noun(ss(g.group_type.member_noun()));
        ui.set_shares_label(ss(g.group_type.shares_label()));
        let labels = g.day_labels();
        ui.set_trip_range(ss(format!(
            "{} to {} • {} day{}",
            labels.first().cloned().unwrap_or_default(),
            labels.last().cloned().unwrap_or_default(),
            g.num_days,
            if g.num_days == 1 { "" } else { "s" }
        )));

        // People.
        let people: Vec<PersonRow> = g
            .people
            .iter()
            .map(|p| {
                let days: Vec<ChipRow> = labels
                    .iter()
                    .enumerate()
                    .map(|(i, l)| ChipRow {
                        id: i as i32,
                        label: ss(l),
                        on: p.attendance.get(i).copied().unwrap_or(true),
                    })
                    .collect();
                let there = days.iter().filter(|d| d.on).count();
                PersonRow {
                    id: p.id as i32,
                    name: ss(&p.name),
                    description: ss(&p.description),
                    shares: ss(trim_num(p.shares)),
                    days: ModelRc::new(VecModel::from(days)),
                    days_there: ss(format!("{} of {}", there, g.num_days)),
                }
            })
            .collect();
        sync(&self.m.people, people, force_lists);
        sync(
            &self.m.people_names,
            g.people.iter().map(|p| ss(g.person_name(p.id))).collect(),
            false,
        );

        // Expenses list.
        let rows: Vec<ExpenseRow> = g
            .expenses
            .iter()
            .zip(&r.expenses)
            .map(|(e, er)| {
                let payer = er
                    .payer
                    .map(|p| g.person_name(p))
                    .unwrap_or_else(|| "no payer".into());
                let n = e.participants.len();
                let mut sub = format!(
                    "Paid by {} • {} {}",
                    payer,
                    n,
                    if n == 1 { "person" } else { "people" }
                );
                if !e.date.trim().is_empty() {
                    sub = format!("{} • {}", e.date, sub);
                }
                ExpenseRow {
                    id: e.id as i32,
                    description: ss(&er.description),
                    subtitle: ss(sub),
                    total: money(er.total),
                    method: ss(e.method.label()),
                    override_count: g
                        .overrides
                        .iter()
                        .filter(|o| o.scope == OverrideScope::Expense(e.id))
                        .count() as i32,
                    warning: !er.warnings.is_empty(),
                }
            })
            .collect();
        sync(&self.m.expenses, rows, force_lists);

        // Overrides.
        let applied = r.all_overrides();
        let ov_row = |o: &Override| {
            let scope = match o.scope {
                OverrideScope::Group => "Whole group".to_string(),
                OverrideScope::Expense(id) => g
                    .expense(id)
                    .map(|e| e.description.clone())
                    .unwrap_or_else(|| "(removed)".into()),
            };
            let a = applied.iter().find(|(_, a)| a.override_id == o.id);
            OverrideRow {
                id: o.id as i32,
                description: ss(if o.description.trim().is_empty() {
                    "(no description)"
                } else {
                    &o.description
                }),
                detail: ss(format!("{} • {}", g.person_name(o.person), scope)),
                effect: ss(a.map(|(_, a)| a.effect.clone()).unwrap_or_else(|| {
                    format!(
                        "{} — not applied, see Summary notes",
                        calc::describe_override(o, g)
                    )
                })),
                ok: a.is_some(),
            }
        };
        sync(
            &self.m.overrides,
            g.overrides.iter().map(ov_row).collect(),
            false,
        );
        let mut scopes = vec![ss("Whole group (person's total)")];
        scopes.extend(g.expenses.iter().map(|e| {
            ss(format!(
                "{} ({})",
                if e.description.is_empty() {
                    "(untitled)"
                } else {
                    &e.description
                },
                cur.format(e.total())
            ))
        }));
        sync(&self.m.ov_scopes, scopes, false);

        // Contributions.
        let contribs: Vec<ContributionRow> = g
            .contributions
            .iter()
            .map(|c| ContributionRow {
                id: c.id as i32,
                description: ss(&c.description),
                from: ss(&c.from),
                amount: ss(cur.edit_string(c.amount)),
                holder: c
                    .received_by
                    .and_then(|h| g.people.iter().position(|p| p.id == h))
                    .map(|i| i as i32)
                    .unwrap_or(-1),
            })
            .collect();
        sync(&self.m.contributions, contribs, force_lists);

        // Summary.
        ui.set_total_spent(money(r.total_spent));
        ui.set_total_contrib(money(r.total_contributions));
        ui.set_people_count(ss(g.people.len().to_string()));
        ui.set_show_credit(r.total_contributions != 0);
        let summary: Vec<SummaryRow> = r
            .people
            .iter()
            .map(|p| SummaryRow {
                name: ss(&p.name),
                paid: money(p.paid),
                share: money(p.owed),
                credit: money(p.credit),
                balance: ss(if p.balance > 0 {
                    format!("gets back {}", cur.format(p.balance))
                } else if p.balance < 0 {
                    format!("owes {}", cur.format(-p.balance))
                } else {
                    "settled".into()
                }),
                sign: p.balance.signum() as i32,
            })
            .collect();
        sync(&self.m.summary, summary, false);
        sync(
            &self.m.transfers,
            r.transfers
                .iter()
                .map(|t| TransferRow {
                    from: ss(g.person_name(t.from)),
                    to: ss(g.person_name(t.to)),
                    amount: money(t.amount),
                })
                .collect(),
            false,
        );
        let fmt_round = |n: &calc::RoundingNote| {
            let amt = if n.amount > 0 {
                format!("+{}", cur.minor_name(n.amount))
            } else {
                cur.minor_name(n.amount)
            };
            ss(format!(
                "{} {} — {}",
                g.person_name(n.person),
                amt,
                n.context
            ))
        };
        sync(
            &self.m.rounding,
            r.all_rounding().into_iter().map(fmt_round).collect(),
            false,
        );
        sync(&self.m.warnings, r.warnings.iter().map(ss).collect(), false);

        // Expense editor.
        if let Some(e) = st.editing.and_then(|id| g.expense(id)) {
            let er = r.expenses.iter().find(|x| x.expense_id == e.id).unwrap();
            let methods = methods_for(g, e);
            sync(
                &self.m.edit_methods,
                methods
                    .iter()
                    .map(|m| MethodRow {
                        title: ss(m.label()),
                        detail: ss(m.explain()),
                    })
                    .collect(),
                force_edit,
            );
            ui.set_edit_method_index(
                methods.iter().position(|m| *m == e.method).unwrap_or(0) as i32
            );
            ui.set_edit_method_key(ss(method_key(e.method)));
            ui.set_edit_total(money(e.total()));
            ui.set_edit_subtotal(money(e.item_subtotal()));
            ui.set_edit_tip_amount(money(e.tip_amount()));
            ui.set_edit_value_label(ss(match e.method {
                Method::Shares => "shares",
                Method::Percent => "%",
                Method::Exact => "amount",
                _ => "",
            }));

            let my_ovs: Vec<&Override> = g
                .overrides
                .iter()
                .filter(|o| o.scope == OverrideScope::Expense(e.id))
                .collect();
            let shares: Vec<ShareRow> = g
                .people
                .iter()
                .map(|p| {
                    let included = e.participants.contains(&p.id);
                    let line = er.lines.iter().find(|l| l.person == p.id);
                    let value = match e.method {
                        Method::Shares => {
                            trim_num(e.shares.get(&p.id).copied().unwrap_or(p.shares))
                        }
                        Method::Percent => e
                            .percents
                            .get(&p.id)
                            .map(|v| trim_num(*v))
                            .unwrap_or_default(),
                        Method::Exact => e
                            .exact
                            .get(&p.id)
                            .map(|v| cur.edit_string(*v))
                            .unwrap_or_default(),
                        _ => String::new(),
                    };
                    let mut notes: Vec<String> = vec![];
                    for o in my_ovs.iter().filter(|o| o.person == p.id) {
                        let eff = er
                            .overrides
                            .iter()
                            .find(|a| a.override_id == o.id)
                            .map(|a| a.effect.clone())
                            .unwrap_or_else(|| "not applied".into());
                        notes.push(format!("Override: {} — {}", o.description, eff));
                    }
                    for rn in er.rounding.iter().filter(|rn| rn.person == p.id) {
                        notes.push(format!(
                            "{}{} rounding",
                            if rn.amount > 0 { "+" } else { "" },
                            cur.minor_name(rn.amount)
                        ));
                    }
                    if er.payer == Some(p.id) {
                        notes.push("Paid this expense".into());
                    }
                    ShareRow {
                        person_id: p.id as i32,
                        name: ss(g.person_name(p.id)),
                        included,
                        value: ss(value),
                        base: line.map(|l| money(l.base)).unwrap_or_default(),
                        amount: if included {
                            line.map(|l| money(l.amount)).unwrap_or_else(|| ss("—"))
                        } else {
                            ss("—")
                        },
                        note: ss(notes.join(" • ")),
                        overridden: line.map(|l| l.overridden).unwrap_or(false),
                        excluded: line.map(|l| l.excluded).unwrap_or(false),
                    }
                })
                .collect();
            sync(&self.m.edit_shares, shares, force_edit);

            let (footer, bad) = match e.method {
                Method::Percent => {
                    let s: f64 = e
                        .participants
                        .iter()
                        .map(|p| e.percents.get(p).copied().unwrap_or(0.0))
                        .sum();
                    (
                        format!("Percents total {}% of 100%", trim_num(s)),
                        (s - 100.0).abs() > 0.005,
                    )
                }
                Method::Exact => {
                    let s: i64 = e
                        .participants
                        .iter()
                        .map(|p| e.exact.get(p).copied().unwrap_or(0))
                        .sum();
                    (
                        format!(
                            "Exact amounts total {} of {}",
                            cur.format(s),
                            cur.format(e.total())
                        ),
                        s != e.total(),
                    )
                }
                Method::Shares => {
                    let s: f64 = e
                        .participants
                        .iter()
                        .map(|p| {
                            e.shares
                                .get(p)
                                .copied()
                                .or_else(|| g.person(*p).map(|x| x.shares))
                                .unwrap_or(1.0)
                        })
                        .sum();
                    (format!("{} shares in total", trim_num(s)), false)
                }
                _ => (String::new(), false),
            };
            ui.set_edit_shares_footer(ss(footer));
            ui.set_edit_shares_footer_bad(bad);

            let items: Vec<ItemRow> = e
                .items
                .iter()
                .map(|it| ItemRow {
                    description: ss(&it.description),
                    amount: ss(cur.edit_string(it.amount)),
                    who: ModelRc::new(VecModel::from(
                        g.people
                            .iter()
                            .filter(|p| e.participants.contains(&p.id))
                            .map(|p| ChipRow {
                                id: p.id as i32,
                                label: ss(g.person_name(p.id)),
                                on: it.shared_by.contains(&p.id),
                            })
                            .collect::<Vec<_>>(),
                    )),
                })
                .collect();
            sync(&self.m.edit_items, items, force_edit);
            let days: Vec<ChipRow> = labels
                .iter()
                .enumerate()
                .map(|(i, l)| ChipRow {
                    id: i as i32,
                    label: ss(l),
                    on: e.days.is_empty() || e.days.get(i).copied().unwrap_or(false),
                })
                .collect();
            sync(&self.m.edit_days, days, force_edit);
            sync(
                &self.m.edit_overrides,
                my_ovs.iter().map(|o| ov_row(o)).collect(),
                false,
            );
            sync(
                &self.m.edit_warnings,
                er.warnings.iter().map(ss).collect(),
                false,
            );
            sync(
                &self.m.edit_rounding,
                er.rounding.iter().map(fmt_round).collect(),
                false,
            );
        }
    }

    fn open_expense_ui(&self, id: Id) {
        let ui = self.ui();
        {
            let mut st = self.st.borrow_mut();
            st.editing = Some(id);
            let g = &st.group;
            let Some(e) = g.expense(id) else { return };
            let cur = g.currency;
            ui.set_edit_description(ss(&e.description));
            ui.set_edit_date(ss(&e.date));
            ui.set_edit_amount(ss(cur.edit_string(e.amount)));
            ui.set_edit_payer_index(
                e.paid_by
                    .and_then(|p| g.people.iter().position(|x| x.id == p))
                    .map(|i| i as i32)
                    .unwrap_or(-1),
            );
            ui.set_edit_tax(ss(cur.edit_string(e.tax)));
            let (mode, val) = match e.tip {
                Tip::None => (0, String::new()),
                Tip::Percent(p) => (1, trim_num(p)),
                Tip::Amount(a) => (2, cur.edit_string(a)),
            };
            ui.set_edit_tip_mode(mode);
            ui.set_edit_tip_value(ss(val));
        }
        ui.set_editing_expense(true);
        self.refresh(false, true);
    }

    pub fn open_path(&self, path: PathBuf) {
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| Group::from_json(&s))
        {
            Ok(g) => {
                self.set_group(g, Some(path.clone()));
                self.toast(format!("Opened {}", path.display()));
            }
            Err(e) => self.toast(format!("Could not open file: {}", e)),
        }
    }

    fn default_dir(&self) -> Option<PathBuf> {
        self.st
            .borrow()
            .path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    }

    fn safe_name(&self) -> String {
        let n = self.st.borrow().group.name.trim().to_string();
        let n: String = n
            .chars()
            .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
            .collect();
        if n.is_empty() {
            "group".into()
        } else {
            n
        }
    }

    fn save(&self, ask: bool) {
        let mut path = self.st.borrow().path.clone();
        if ask || path.is_none() {
            let mut dlg = rfd::FileDialog::new()
                .set_title("Save group")
                .add_filter("GroupKeep group", &[FILE_EXTENSION])
                .set_file_name(format!("{}.{}", self.safe_name(), FILE_EXTENSION));
            if let Some(d) = self.default_dir() {
                dlg = dlg.set_directory(d);
            }
            path = dlg.save_file();
        }
        let Some(mut p) = path else { return };
        if p.extension().is_none() {
            p.set_extension(FILE_EXTENSION);
        }
        let json = self.st.borrow().group.to_json();
        match std::fs::write(&p, json) {
            Ok(()) => {
                {
                    let mut st = self.st.borrow_mut();
                    st.path = Some(p.clone());
                    st.dirty = false;
                }
                self.refresh(false, false);
                self.toast(format!("Saved {}", p.display()));
            }
            Err(e) => self.toast(format!("Could not save: {}", e)),
        }
    }

    fn export_pdf(&self) {
        let mut dlg = rfd::FileDialog::new()
            .set_title("Export PDF")
            .add_filter("PDF", &["pdf"])
            .set_file_name(format!("{}.pdf", self.safe_name()));
        if let Some(d) = self.default_dir() {
            dlg = dlg.set_directory(d);
        }
        let Some(mut p) = dlg.save_file() else { return };
        if p.extension().is_none() {
            p.set_extension("pdf");
        }
        let bytes = {
            let st = self.st.borrow();
            let r = calc::compute(&st.group);
            groupkeep_core::pdf::render(&st.group, &r)
        };
        match bytes.and_then(|b| std::fs::write(&p, b).map_err(|e| e.to_string())) {
            Ok(()) => self.toast(format!("PDF saved: {}", p.display())),
            Err(e) => self.toast(format!("Could not export PDF: {}", e)),
        }
    }

    fn open_dialog(&self) {
        let mut dlg = rfd::FileDialog::new()
            .set_title("Open group")
            .add_filter("GroupKeep group", &[FILE_EXTENSION]);
        if let Some(d) = self.default_dir() {
            dlg = dlg.set_directory(d);
        }
        if let Some(p) = dlg.pick_file() {
            self.open_path(p);
        }
    }

    fn ask(&self, what: Pending, text: &str, yes: &str) {
        self.st.borrow_mut().pending = Some(what);
        let ui = self.ui();
        ui.set_confirm_text(ss(text));
        ui.set_confirm_yes_label(ss(yes));
        ui.set_confirm_open(true);
    }

    fn run_pending(&self, p: Pending) {
        match p {
            Pending::NewGroup => {
                self.set_group(Group::default(), None);
                self.ui().set_page(0);
            }
            Pending::OpenFile => self.open_dialog(),
            Pending::Quit => {
                let _ = slint::quit_event_loop();
            }
            Pending::DeleteExpense => {
                let id = self.st.borrow().editing;
                if let Some(id) = id {
                    {
                        let mut st = self.st.borrow_mut();
                        st.group.remove_expense(id);
                        st.editing = None;
                        st.dirty = true;
                    }
                    self.ui().set_editing_expense(false);
                    self.refresh(true, true);
                }
            }
        }
    }

    // ------------------------------------------------------------------ editing helpers

    /// Mutate the expense being edited, then refresh.
    fn with_expense(&self, force_edit: bool, f: impl FnOnce(&mut Expense, &Group)) {
        {
            let mut st = self.st.borrow_mut();
            let Some(id) = st.editing else { return };
            let snapshot = st.group.clone();
            if let Some(e) = st.group.expense_mut(id) {
                f(e, &snapshot);
            }
            st.dirty = true;
        }
        self.refresh(false, force_edit);
    }

    fn with_group(&self, force_lists: bool, f: impl FnOnce(&mut Group)) {
        {
            let mut st = self.st.borrow_mut();
            f(&mut st.group);
            st.dirty = true;
        }
        self.refresh(force_lists, false);
    }

    fn read_expense_fields(&self) {
        let ui = self.ui();
        let desc = ui.get_edit_description().to_string();
        let date = ui.get_edit_date().to_string();
        let amount = ui.get_edit_amount().to_string();
        let payer = ui.get_edit_payer_index();
        let tax = ui.get_edit_tax().to_string();
        let tip_mode = ui.get_edit_tip_mode();
        let mut tip_val = ui.get_edit_tip_value().to_string();
        if tip_mode == 1 && tip_val.trim().is_empty() {
            tip_val = "18".into();
            ui.set_edit_tip_value(ss(&tip_val));
        }
        self.with_expense(false, |e, g| {
            let cur = g.currency;
            e.description = desc;
            e.date = date;
            if let Some(a) = cur.parse(&amount) {
                e.amount = a;
            }
            e.paid_by = if payer >= 0 {
                g.people.get(payer as usize).map(|p| p.id)
            } else {
                None
            };
            if let Some(t) = cur.parse(&tax) {
                e.tax = t;
            }
            e.tip = match tip_mode {
                1 => Tip::Percent(parse_f64(&tip_val).unwrap_or(0.0)),
                2 => Tip::Amount(cur.parse(&tip_val).unwrap_or(0)),
                _ => Tip::None,
            };
        });
    }

    fn open_override_form(&self, existing: Option<Id>, for_expense: bool) {
        let ui = self.ui();
        let st = self.st.borrow();
        let g = &st.group;
        let cur = g.currency;
        let scope_index = |scope: OverrideScope| match scope {
            OverrideScope::Group => 0,
            OverrideScope::Expense(id) => g
                .expenses
                .iter()
                .position(|e| e.id == id)
                .map(|i| i as i32 + 1)
                .unwrap_or(0),
        };
        match existing.and_then(|id| g.overrides.iter().find(|o| o.id == id)) {
            Some(o) => {
                ui.set_ov_is_new(false);
                ui.set_ov_description(ss(&o.description));
                ui.set_ov_person_index(
                    g.people
                        .iter()
                        .position(|p| p.id == o.person)
                        .map(|i| i as i32)
                        .unwrap_or(-1),
                );
                ui.set_ov_scope_index(scope_index(o.scope));
                ui.set_ov_kind_index(o.kind.index() as i32);
                ui.set_ov_value(ss(match o.kind {
                    OverrideKind::Fixed(a) | OverrideKind::Adjust(a) => {
                        cur.format_number(a).replace(',', "")
                    }
                    OverrideKind::AdjustPercent(p) => trim_num(p),
                    OverrideKind::Exclude => String::new(),
                }));
            }
            None => {
                ui.set_ov_is_new(true);
                ui.set_ov_description(SharedString::new());
                ui.set_ov_person_index(-1);
                ui.set_ov_scope_index(match (for_expense, st.editing) {
                    (true, Some(id)) => scope_index(OverrideScope::Expense(id)),
                    _ => 0,
                });
                ui.set_ov_kind_index(0);
                ui.set_ov_value(SharedString::new());
            }
        }
        ui.set_ov_error(SharedString::new());
        drop(st);
        self.st.borrow_mut().ov_editing = existing;
        ui.set_ov_open(true);
    }

    fn save_override(&self) {
        let ui = self.ui();
        let desc = ui.get_ov_description().trim().to_string();
        let pi = ui.get_ov_person_index();
        let si = ui.get_ov_scope_index();
        let ki = ui.get_ov_kind_index();
        let val = ui.get_ov_value().to_string();
        let result: Result<Override, String> = (|| {
            let st = self.st.borrow();
            let g = &st.group;
            if desc.is_empty() {
                return Err("Please describe why this override exists.".into());
            }
            let person = g
                .people
                .get(pi.max(0) as usize)
                .filter(|_| pi >= 0)
                .ok_or("Pick who this override is for.")?
                .id;
            let scope = if si <= 0 {
                OverrideScope::Group
            } else {
                OverrideScope::Expense(
                    g.expenses
                        .get(si as usize - 1)
                        .ok_or("Pick what it applies to.")?
                        .id,
                )
            };
            let kind = match ki {
                0 => OverrideKind::Fixed(
                    g.currency
                        .parse(&val)
                        .filter(|_| !val.trim().is_empty())
                        .ok_or("Enter the amount they pay.")?,
                ),
                1 => OverrideKind::Adjust(
                    g.currency
                        .parse(&val)
                        .filter(|v| *v != 0)
                        .ok_or("Enter an amount (use - for less).")?,
                ),
                2 => OverrideKind::AdjustPercent(
                    parse_f64(&val)
                        .filter(|v| *v != 0.0)
                        .ok_or("Enter a percent (use - for less).")?,
                ),
                _ => OverrideKind::Exclude,
            };
            let o = Override {
                id: st.ov_editing.unwrap_or(0),
                description: desc.clone(),
                person,
                scope,
                kind,
            };
            // upgrade-resilience-1 L11: a share larger than the whole expense is refused here.
            calc::check_override(g, &o)?;
            Ok(o)
        })();
        match result {
            Err(e) => ui.set_ov_error(ss(e)),
            Ok(mut o) => {
                {
                    let mut st = self.st.borrow_mut();
                    if o.id == 0 {
                        o.id = st.group.new_id();
                        st.group.overrides.push(o);
                    } else if let Some(x) = st.group.overrides.iter_mut().find(|x| x.id == o.id) {
                        *x = o;
                    }
                    st.dirty = true;
                }
                ui.set_ov_open(false);
                self.refresh(false, false);
                self.toast("Override saved");
            }
        }
    }

    // ------------------------------------------------------------------ wiring

    fn wire(ctl: &Rc<Controller>, ui: &AppWindow) {
        macro_rules! on {
            ($setter:ident, |$c:ident $(, $arg:ident : $t:ty)*| $body:expr) => {{
                let w = Rc::downgrade(ctl);
                ui.$setter(move |$($arg: $t),*| {
                    if let Some($c) = w.upgrade() {
                        $body
                    }
                });
            }};
        }

        // Group
        on!(on_group_changed, |c| {
            let ui = c.ui();
            let (n, d, s) = (
                ui.get_group_name().to_string(),
                ui.get_group_description().to_string(),
                ui.get_start_date().to_string(),
            );
            c.with_group(false, |g| {
                g.name = n;
                g.description = d;
                g.start_date = s;
            });
        });
        on!(on_group_type_selected, |c, i: i32| {
            c.with_group(true, |g| {
                g.group_type = GroupType::from_index(i as usize);
                if g.group_type.uses_days() {
                    let n = g.num_days.max(1);
                    g.set_num_days(n);
                }
            });
        });
        on!(on_currency_selected, |c, i: i32| {
            c.with_group(true, |g| g.set_currency(Currency::from_index(i as usize)));
        });
        on!(on_num_days_changed, |c, v: i32| {
            c.with_group(false, |g| g.set_num_days(v.max(1) as u32));
        });
        on!(on_new_group, |c| {
            if c.st.borrow().dirty {
                c.ask(
                    Pending::NewGroup,
                    "Start a new group? Unsaved changes will be lost.",
                    "Discard",
                );
            } else {
                c.run_pending(Pending::NewGroup);
            }
        });
        on!(on_open_file, |c| {
            if c.st.borrow().dirty {
                c.ask(
                    Pending::OpenFile,
                    "Open another group? Unsaved changes will be lost.",
                    "Discard",
                );
            } else {
                c.open_dialog();
            }
        });
        on!(on_save_file, |c| c.save(false));
        on!(on_save_file_as, |c| c.save(true));
        on!(on_export_pdf, |c| c.export_pdf());

        // People
        on!(on_add_person, |c, name: SharedString| {
            let n = name.trim().to_string();
            c.with_group(true, |g| {
                let n = if n.is_empty() {
                    format!("{} {}", g.group_type.member_noun(), g.people.len() + 1)
                } else {
                    n
                };
                g.add_person(&n);
            });
        });
        on!(on_remove_person, |c, id: i32| c
            .with_group(true, |g| g.remove_person(id as Id)));
        on!(on_person_name_edited, |c, id: i32, t: SharedString| {
            c.with_group(false, |g| {
                if let Some(p) = g.people.iter_mut().find(|p| p.id == id as Id) {
                    p.name = t.to_string();
                }
            })
        });
        on!(on_person_desc_edited, |c, id: i32, t: SharedString| {
            c.with_group(false, |g| {
                if let Some(p) = g.people.iter_mut().find(|p| p.id == id as Id) {
                    p.description = t.to_string();
                }
            })
        });
        on!(on_person_shares_edited, |c, id: i32, t: SharedString| {
            if let Some(v) = parse_f64(&t).filter(|v| *v >= 0.0) {
                c.with_group(false, |g| {
                    if let Some(p) = g.people.iter_mut().find(|p| p.id == id as Id) {
                        p.shares = v;
                    }
                })
            }
        });
        on!(on_person_day_toggled, |c, id: i32, d: i32| {
            c.with_group(false, |g| {
                let n = g.num_days as usize;
                if let Some(p) = g.people.iter_mut().find(|p| p.id == id as Id) {
                    p.attendance.resize(n, true);
                    if let Some(x) = p.attendance.get_mut(d as usize) {
                        *x = !*x;
                    }
                }
            })
        });

        // Expenses
        on!(on_add_expense, |c| {
            let id = {
                let mut st = c.st.borrow_mut();
                st.dirty = true;
                st.group.add_expense()
            };
            c.refresh(true, false);
            c.open_expense_ui(id);
        });
        on!(on_open_expense, |c, id: i32| c.open_expense_ui(id as Id));
        on!(on_close_expense, |c| {
            c.st.borrow_mut().editing = None;
            c.ui().set_editing_expense(false);
            c.refresh(true, false);
        });
        on!(on_delete_expense, |c| c.ask(
            Pending::DeleteExpense,
            "Delete this expense and its overrides?",
            "Delete"
        ));
        on!(on_expense_field_changed, |c| c.read_expense_fields());
        on!(on_method_selected, |c, i: i32| {
            c.with_expense(true, |e, g| {
                if let Some(m) = methods_for(g, e).get(i as usize).copied() {
                    if m == Method::ByItem && e.items.is_empty() {
                        e.items.push(Item {
                            description: String::new(),
                            amount: e.amount,
                            shared_by: vec![],
                        });
                    }
                    if m != Method::ByItem && e.method == Method::ByItem && e.amount == 0 {
                        e.amount = e.total();
                    }
                    e.method = m;
                }
            });
            // Amount field visibility may have changed; show the current amount.
            let st = c.st.borrow();
            if let Some(e) = st.editing.and_then(|id| st.group.expense(id)) {
                c.ui()
                    .set_edit_amount(ss(st.group.currency.edit_string(e.amount)));
            }
        });
        on!(on_participant_toggled, |c, pid: i32| {
            c.with_expense(false, |e, _| {
                let pid = pid as Id;
                if let Some(i) = e.participants.iter().position(|p| *p == pid) {
                    e.participants.remove(i);
                } else {
                    e.participants.push(pid);
                }
            })
        });
        on!(on_all_participants, |c, all: bool| {
            c.with_expense(true, |e, g| {
                e.participants = if all {
                    g.people.iter().map(|p| p.id).collect()
                } else {
                    vec![]
                };
            })
        });
        on!(on_share_value_edited, |c, pid: i32, t: SharedString| {
            c.with_expense(false, |e, g| {
                let pid = pid as Id;
                let empty = t.trim().is_empty();
                match e.method {
                    Method::Shares => match parse_f64(&t) {
                        Some(v) if v >= 0.0 => {
                            e.shares.insert(pid, v);
                        }
                        _ if empty => {
                            e.shares.remove(&pid);
                        }
                        _ => {}
                    },
                    Method::Percent => match parse_f64(&t) {
                        Some(v) => {
                            e.percents.insert(pid, v);
                        }
                        None if empty => {
                            e.percents.remove(&pid);
                        }
                        None => {}
                    },
                    Method::Exact => match g.currency.parse(&t) {
                        Some(_) if empty => {
                            e.exact.remove(&pid);
                        }
                        Some(v) => {
                            e.exact.insert(pid, v);
                        }
                        None => {}
                    },
                    _ => {}
                }
            })
        });
        on!(on_add_item, |c| {
            c.with_expense(true, |e, _| {
                e.items.push(Item {
                    description: String::new(),
                    amount: 0,
                    shared_by: vec![],
                })
            })
        });
        on!(on_remove_item, |c, i: i32| {
            c.with_expense(true, |e, _| {
                if (i as usize) < e.items.len() {
                    e.items.remove(i as usize);
                }
            })
        });
        on!(on_item_desc_edited, |c, i: i32, t: SharedString| {
            c.with_expense(false, |e, _| {
                if let Some(it) = e.items.get_mut(i as usize) {
                    it.description = t.to_string();
                }
            })
        });
        on!(on_item_amount_edited, |c, i: i32, t: SharedString| {
            c.with_expense(false, |e, g| {
                if let (Some(it), Some(v)) = (e.items.get_mut(i as usize), g.currency.parse(&t)) {
                    it.amount = v;
                }
            })
        });
        on!(on_item_who_toggled, |c, i: i32, pid: i32| {
            c.with_expense(false, |e, _| {
                if let Some(it) = e.items.get_mut(i as usize) {
                    let pid = pid as Id;
                    if let Some(k) = it.shared_by.iter().position(|p| *p == pid) {
                        it.shared_by.remove(k);
                    } else {
                        it.shared_by.push(pid);
                    }
                }
            })
        });
        on!(on_expense_day_toggled, |c, d: i32| {
            c.with_expense(false, |e, g| {
                let n = g.num_days.max(1) as usize;
                if e.days.is_empty() {
                    e.days = vec![true; n];
                }
                e.days.resize(n, false);
                if let Some(x) = e.days.get_mut(d as usize) {
                    *x = !*x;
                }
                if e.days.iter().all(|x| *x) {
                    e.days.clear();
                }
            })
        });

        // Overrides
        on!(on_new_override, |c, for_expense: bool| c
            .open_override_form(None, for_expense));
        on!(on_edit_override, |c, id: i32| c
            .open_override_form(Some(id as Id), false));
        on!(on_save_override, |c| c.save_override());
        on!(on_delete_override, |c| {
            let id = c.st.borrow().ov_editing;
            if let Some(id) = id {
                c.with_group(false, |g| g.overrides.retain(|o| o.id != id));
            }
            c.ui().set_ov_open(false);
            c.toast("Override deleted");
        });
        on!(on_ov_changed, |c| c.ui().set_ov_error(SharedString::new()));

        // Contributions
        on!(on_add_contribution, |c| {
            c.with_group(true, |g| {
                let id = g.new_id();
                let holder = g.people.first().map(|p| p.id);
                g.contributions.push(Contribution {
                    id,
                    description: String::new(),
                    from: String::new(),
                    amount: 0,
                    received_by: holder,
                });
            })
        });
        on!(on_remove_contribution, |c, id: i32| c
            .with_group(true, |g| g
                .contributions
                .retain(|x| x.id != id as Id)));
        on!(
            on_contribution_edited,
            |c, id: i32, d: SharedString, f: SharedString, a: SharedString| {
                c.with_group(false, |g| {
                    let cur = g.currency;
                    if let Some(x) = g.contributions.iter_mut().find(|x| x.id == id as Id) {
                        x.description = d.to_string();
                        x.from = f.to_string();
                        if let Some(v) = cur.parse(&a) {
                            x.amount = v;
                        }
                    }
                })
            }
        );
        on!(on_contribution_holder, |c, id: i32, idx: i32| {
            c.with_group(false, |g| {
                let holder = if idx >= 0 {
                    g.people.get(idx as usize).map(|p| p.id)
                } else {
                    None
                };
                if let Some(x) = g.contributions.iter_mut().find(|x| x.id == id as Id) {
                    x.received_by = holder;
                }
            })
        });

        // Confirm dialog
        on!(on_confirm_yes, |c| {
            c.ui().set_confirm_open(false);
            let p = c.st.borrow_mut().pending.take();
            if let Some(p) = p {
                c.run_pending(p);
            }
        });
        on!(on_confirm_no, |c| {
            c.ui().set_confirm_open(false);
            c.st.borrow_mut().pending = None;
        });

        // Closing with unsaved changes.
        let w = Rc::downgrade(ctl);
        ui.window().on_close_requested(move || {
            if let Some(c) = w.upgrade() {
                if c.st.borrow().dirty {
                    c.ask(
                        Pending::Quit,
                        "Quit without saving? Unsaved changes will be lost.",
                        "Quit",
                    );
                    return slint::CloseRequestResponse::KeepWindowShown;
                }
            }
            slint::CloseRequestResponse::HideWindow
        });
    }
}
