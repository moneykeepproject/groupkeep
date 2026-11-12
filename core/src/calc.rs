//! Division engine: turns a [`Group`] into a [`Report`] of who owes what and who pays whom.
//!
//! Rules, in order:
//! 1. Each expense is split by its method among its participants (minus anyone excluded by an override).
//! 2. Expense overrides are applied; the difference is re-split among the remaining participants
//!    in proportion to their shares.
//! 3. Group overrides are applied to each person's total the same way.
//! 4. Donations / sponsor money lowers everyone's total in proportion to what they owe.
//!    An override whose amount is more than its expense's total is refused when it is entered
//!    ([`check_override`]). If fixed shares together still exceed the total, the others' shares
//!    are 0, never negative, and the expense notes [`SHARES_OVER_TOTAL`] (upgrade-resilience-1
//!    L11).
//! 5. Leftover pennies from rounding always go to the payer, and are reported.
//! 6. Settle-up uses the fewest practical payments (largest debtor pays largest creditor).

use crate::model::*;
use crate::money::Currency;
use std::collections::BTreeMap;

/// Shown when an override's amount is more than its expense's total (upgrade-resilience-1 L11).
pub const SHARE_TOO_LARGE: &str = "This share is more than the whole expense.";

/// An expense's note while its fixed shares add up to more than its total (upgrade-resilience-1
/// L11); the other participants' shares are 0 until it is fixed.
pub const SHARES_OVER_TOTAL: &str = "Shares add up to more than the total";

/// Checks an override when it is entered: on one expense, an amount the person pays ("Pays
/// exactly") or adds ("Add / subtract amount") that is more than the expense's whole total is
/// refused with [`SHARE_TOO_LARGE`].
pub fn check_override(g: &Group, o: &Override) -> Result<(), String> {
    let OverrideScope::Expense(id) = o.scope else {
        return Ok(());
    };
    let Some(e) = g.expense(id) else {
        return Ok(());
    };
    match o.kind {
        OverrideKind::Fixed(a) | OverrideKind::Adjust(a) if a > e.total() => {
            Err(SHARE_TOO_LARGE.into())
        }
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoundingNote {
    pub person: Id,
    /// Extra minor units this person pays (can be negative).
    pub amount: i64,
    pub context: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppliedOverride {
    pub override_id: Id,
    pub person: Id,
    pub description: String,
    /// Human summary: "Pays exactly $20.00 (was $33.34)".
    pub effect: String,
    pub before: i64,
    pub after: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShareLine {
    pub person: Id,
    /// From the division method alone.
    pub base: i64,
    /// After overrides.
    pub amount: i64,
    /// Whether an override on this expense targeted this person.
    pub overridden: bool,
    pub excluded: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExpenseResult {
    pub expense_id: Id,
    pub description: String,
    pub date: String,
    pub method: Method,
    pub payer: Option<Id>,
    pub total: i64,
    pub lines: Vec<ShareLine>,
    pub overrides: Vec<AppliedOverride>,
    pub rounding: Vec<RoundingNote>,
    pub warnings: Vec<String>,
    /// Counted in totals (false if no payer or zero amount).
    pub counted: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PersonSummary {
    pub id: Id,
    pub name: String,
    pub paid: i64,
    /// Sum of expense shares (after expense overrides).
    pub expense_share: i64,
    /// After group overrides.
    pub owed: i64,
    /// Portion of donations credited to this person.
    pub credit: i64,
    /// owed - credit
    pub net_owed: i64,
    /// Donation money this person is holding.
    pub holding: i64,
    /// paid - net_owed - holding. Positive = should receive money.
    pub balance: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transfer {
    pub from: Id,
    pub to: Id,
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    pub currency: Currency,
    pub expenses: Vec<ExpenseResult>,
    pub group_overrides: Vec<AppliedOverride>,
    pub group_rounding: Vec<RoundingNote>,
    pub people: Vec<PersonSummary>,
    pub transfers: Vec<Transfer>,
    pub total_spent: i64,
    pub total_contributions: i64,
    pub warnings: Vec<String>,
}

impl Report {
    pub fn all_overrides(&self) -> Vec<(&str, &AppliedOverride)> {
        let mut v: Vec<(&str, &AppliedOverride)> = vec![];
        for e in &self.expenses {
            for o in &e.overrides {
                v.push((e.description.as_str(), o));
            }
        }
        for o in &self.group_overrides {
            v.push(("Whole group", o));
        }
        v
    }

    pub fn all_rounding(&self) -> Vec<&RoundingNote> {
        self.expenses
            .iter()
            .flat_map(|e| e.rounding.iter())
            .chain(self.group_rounding.iter())
            .collect()
    }

    pub fn person(&self, id: Id) -> Option<&PersonSummary> {
        self.people.iter().find(|p| p.id == id)
    }
}

/// Split `total` across `weights` proportionally, in whole minor units.
/// Every share is rounded down; the leftover units go to `payer` (or the first person if the
/// payer is not in the list). Returns the shares and the rounding leftover, if any.
/// A person and an amount in minor units.
pub type Amount = (Id, i64);

pub fn allocate(
    total: i64,
    weights: &[(Id, f64)],
    payer: Option<Id>,
) -> (Vec<Amount>, Option<Amount>) {
    if weights.is_empty() {
        return (vec![], None);
    }
    let positive: f64 = weights.iter().map(|(_, w)| w.max(0.0)).sum();
    let use_equal = positive <= 1e-12;
    let wsum = if use_equal {
        weights.len() as f64
    } else {
        positive
    };
    let mut out: Vec<(Id, i64)> = weights
        .iter()
        .map(|(id, w)| {
            let w = if use_equal { 1.0 } else { w.max(0.0) };
            let raw = total as f64 * w / wsum;
            (*id, (raw + 1e-7).floor() as i64)
        })
        .collect();
    let leftover = total - out.iter().map(|(_, a)| a).sum::<i64>();
    if leftover == 0 {
        return (out, None);
    }
    let idx = payer
        .and_then(|p| out.iter().position(|(id, _)| *id == p))
        .unwrap_or(0);
    out[idx].1 += leftover;
    let who = out[idx].0;
    (out, Some((who, leftover)))
}

fn describe_kind(kind: &OverrideKind, cur: Currency) -> String {
    match kind {
        OverrideKind::Fixed(a) => format!("Pays exactly {}", cur.format(*a)),
        OverrideKind::Adjust(a) if *a >= 0 => format!("Pays {} more", cur.format(*a)),
        OverrideKind::Adjust(a) => format!("Pays {} less", cur.format(-*a)),
        OverrideKind::AdjustPercent(p) if *p >= 0.0 => format!("Pays {}% more", trim_num(*p)),
        OverrideKind::AdjustPercent(p) => format!("Pays {}% less", trim_num(-*p)),
        OverrideKind::Exclude => "Pays nothing (excluded)".to_string(),
    }
}

pub fn trim_num(v: f64) -> String {
    let s = format!("{:.2}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn describe_override(o: &Override, g: &Group) -> String {
    describe_kind(&o.kind, g.currency)
}

/// Apply one non-exclude override to `shares`. `locked` people (already given a fixed amount)
/// do not absorb the difference. Returns (before, after, rounding).
fn apply_one(
    shares: &mut Vec<(Id, i64)>,
    person: Id,
    kind: &OverrideKind,
    locked: &[Id],
    payer: Option<Id>,
) -> Result<(i64, i64, Option<Amount>), String> {
    if !shares.iter().any(|(id, _)| *id == person) {
        shares.push((person, 0));
    }
    let cur = shares.iter().find(|(id, _)| *id == person).unwrap().1;
    let target = match kind {
        OverrideKind::Fixed(a) => *a,
        OverrideKind::Adjust(d) => cur + d,
        OverrideKind::AdjustPercent(p) => cur + (cur as f64 * p / 100.0).round() as i64,
        OverrideKind::Exclude => 0,
    };
    let delta = target - cur;
    if delta == 0 {
        return Ok((cur, cur, None));
    }
    let others: Vec<(Id, f64)> = shares
        .iter()
        .filter(|(id, _)| *id != person && !locked.contains(id))
        .map(|(id, a)| (*id, *a as f64))
        .collect();
    if others.is_empty() {
        return Err("no one else is left to cover the difference".into());
    }
    let (absorb, rounding) = allocate(-delta, &others, payer);
    for (id, a) in absorb {
        if let Some(s) = shares.iter_mut().find(|(sid, _)| *sid == id) {
            s.1 += a;
        }
    }
    if let Some(s) = shares.iter_mut().find(|(id, _)| *id == person) {
        s.1 = target;
    }
    Ok((cur, target, rounding))
}

fn base_weights(
    e: &Expense,
    g: &Group,
    parts: &[Id],
    warnings: &mut Vec<String>,
) -> Vec<(Id, f64)> {
    let cur = g.currency;
    let total = e.total();
    match e.method {
        Method::Equal => parts.iter().map(|p| (*p, 1.0)).collect(),
        Method::Shares => parts
            .iter()
            .map(|p| {
                let w = e
                    .shares
                    .get(p)
                    .copied()
                    .or_else(|| g.person(*p).map(|x| x.shares))
                    .unwrap_or(1.0);
                (*p, w.max(0.0))
            })
            .collect(),
        Method::Percent => {
            let v: Vec<(Id, f64)> = parts
                .iter()
                .map(|p| (*p, e.percents.get(p).copied().unwrap_or(0.0)))
                .collect();
            let sum: f64 = v.iter().map(|x| x.1).sum();
            if (sum - 100.0).abs() > 0.005 {
                warnings.push(format!(
                    "Percents total {}%, not 100%. They were scaled to fit the expense.",
                    trim_num(sum)
                ));
            }
            v
        }
        Method::Exact => {
            let v: Vec<(Id, i64)> = parts
                .iter()
                .map(|p| (*p, e.exact.get(p).copied().unwrap_or(0)))
                .collect();
            let sum: i64 = v.iter().map(|x| x.1).sum();
            let diff = total - sum;
            if diff != 0 && !parts.is_empty() {
                warnings.push(format!(
                    "Exact amounts total {}, expense is {}. The {} difference was split equally.",
                    cur.format(sum),
                    cur.format(total),
                    cur.format(diff.abs())
                ));
                let each = diff as f64 / parts.len() as f64;
                v.iter().map(|(id, a)| (*id, *a as f64 + each)).collect()
            } else {
                v.iter().map(|(id, a)| (*id, *a as f64)).collect()
            }
        }
        Method::ByItem => {
            let mut sub: BTreeMap<Id, f64> = parts.iter().map(|p| (*p, 0.0)).collect();
            for it in &e.items {
                let mut who: Vec<Id> = it
                    .shared_by
                    .iter()
                    .copied()
                    .filter(|p| parts.contains(p))
                    .collect();
                if who.is_empty() {
                    who = parts.to_vec();
                }
                let each = it.amount as f64 / who.len().max(1) as f64;
                for p in who {
                    *sub.entry(p).or_default() += each;
                }
            }
            let subtotal: f64 = sub.values().sum();
            let extra = (e.tax + e.tip_amount()) as f64;
            parts
                .iter()
                .map(|p| {
                    let s = sub[p];
                    let share_extra = if subtotal.abs() > 1e-9 {
                        extra * s / subtotal
                    } else {
                        extra / parts.len() as f64
                    };
                    (*p, s + share_extra)
                })
                .collect()
        }
        Method::ByAttendance => {
            let n = g.num_days.max(1) as usize;
            let covered: Vec<usize> = (0..n)
                .filter(|d| e.days.is_empty() || e.days.get(*d).copied().unwrap_or(false))
                .collect();
            let labels = g.day_labels();
            let mut w: BTreeMap<Id, f64> = parts.iter().map(|p| (*p, 0.0)).collect();
            if covered.is_empty() {
                warnings
                    .push("No days are selected for this expense; split equally instead.".into());
                return parts.iter().map(|p| (*p, 1.0)).collect();
            }
            let per_day = total as f64 / covered.len() as f64;
            for d in covered {
                let present: Vec<Id> = parts
                    .iter()
                    .copied()
                    .filter(|p| {
                        g.person(*p)
                            .and_then(|x| x.attendance.get(d).copied())
                            .unwrap_or(true)
                    })
                    .collect();
                let who = if present.is_empty() {
                    warnings.push(format!(
                        "Nobody is marked present on {}; that day was split among everyone.",
                        labels
                            .get(d)
                            .cloned()
                            .unwrap_or_else(|| format!("day {}", d + 1))
                    ));
                    parts.to_vec()
                } else {
                    present
                };
                for p in &who {
                    *w.get_mut(p).unwrap() += per_day / who.len() as f64;
                }
            }
            parts.iter().map(|p| (*p, w[p])).collect()
        }
    }
}

fn compute_expense(e: &Expense, g: &Group, group_excluded: &[Id]) -> ExpenseResult {
    let cur = g.currency;
    let mut warnings = vec![];
    let mut rounding = vec![];
    let mut applied = vec![];
    let total = e.total();

    let my_overrides: Vec<&Override> = g
        .overrides
        .iter()
        .filter(|o| o.scope == OverrideScope::Expense(e.id) && g.person(o.person).is_some())
        .collect();
    let excluded: Vec<Id> = my_overrides
        .iter()
        .filter(|o| o.kind == OverrideKind::Exclude)
        .map(|o| o.person)
        .chain(group_excluded.iter().copied())
        .collect();

    let all_parts: Vec<Id> = e
        .participants
        .iter()
        .copied()
        .filter(|p| g.person(*p).is_some())
        .collect();
    let parts: Vec<Id> = all_parts
        .iter()
        .copied()
        .filter(|p| !excluded.contains(p))
        .collect();

    let payer = e.paid_by.filter(|p| g.person(*p).is_some());
    if payer.is_none() {
        warnings.push("No payer is set, so this expense is left out of the totals.".into());
    }
    if parts.is_empty() && total != 0 {
        warnings.push("Nobody is sharing this expense, so it is left out of the totals.".into());
    }
    let label = if e.description.trim().is_empty() {
        "(untitled)".to_string()
    } else {
        e.description.clone()
    };

    let weights = base_weights(e, g, &parts, &mut warnings);
    let (mut shares, r) = allocate(total, &weights, payer);
    if let Some((p, a)) = r {
        rounding.push(RoundingNote {
            person: p,
            amount: a,
            context: format!("{}: split did not divide evenly", label),
        });
    }
    let base: BTreeMap<Id, i64> = shares.iter().copied().collect();

    // Record exclusions as applied overrides so they are visible.
    for o in my_overrides
        .iter()
        .filter(|o| o.kind == OverrideKind::Exclude)
    {
        applied.push(AppliedOverride {
            override_id: o.id,
            person: o.person,
            description: o.description.clone(),
            effect: describe_kind(&o.kind, cur),
            before: 0,
            after: 0,
        });
    }

    let mut locked: Vec<Id> = vec![];
    for o in my_overrides
        .iter()
        .filter(|o| o.kind != OverrideKind::Exclude)
    {
        if excluded.contains(&o.person) {
            warnings.push(format!(
                "Override \"{}\" ignored: {} is excluded from this expense.",
                o.description,
                g.person_name(o.person)
            ));
            continue;
        }
        match apply_one(&mut shares, o.person, &o.kind, &locked, payer) {
            Ok((before, after, r)) => {
                if let Some((p, a)) = r {
                    rounding.push(RoundingNote {
                        person: p,
                        amount: a,
                        context: format!(
                            "{}: re-splitting after override \"{}\"",
                            label, o.description
                        ),
                    });
                }
                applied.push(AppliedOverride {
                    override_id: o.id,
                    person: o.person,
                    description: o.description.clone(),
                    effect: format!(
                        "{} (was {})",
                        describe_kind(&o.kind, cur),
                        cur.format(before)
                    ),
                    before,
                    after,
                });
                if matches!(o.kind, OverrideKind::Fixed(_)) {
                    locked.push(o.person);
                }
            }
            Err(msg) => warnings.push(format!(
                "Override \"{}\" could not be applied: {}.",
                o.description, msg
            )),
        }
    }
    // Overrides that take more than the total leave the others at 0, never negative
    // (upgrade-resilience-1 L11), and the expense says so until it is fixed.
    let mut over = false;
    for (id, a) in shares.iter_mut() {
        if *a < 0 && !locked.contains(id) {
            *a = 0;
            over = true;
        }
    }
    if over || shares.iter().map(|(_, a)| *a).sum::<i64>() > total {
        warnings.push(SHARES_OVER_TOTAL.into());
    }

    let overridden: Vec<Id> = my_overrides.iter().map(|o| o.person).collect();
    let mut lines: Vec<ShareLine> = shares
        .iter()
        .map(|(id, a)| ShareLine {
            person: *id,
            base: base.get(id).copied().unwrap_or(0),
            amount: *a,
            overridden: overridden.contains(id),
            excluded: false,
        })
        .collect();
    for p in all_parts.iter().filter(|p| excluded.contains(p)) {
        lines.push(ShareLine {
            person: *p,
            base: 0,
            amount: 0,
            overridden: true,
            excluded: true,
        });
    }
    // Keep group member order.
    lines.sort_by_key(|l| {
        g.people
            .iter()
            .position(|p| p.id == l.person)
            .unwrap_or(usize::MAX)
    });

    ExpenseResult {
        expense_id: e.id,
        description: label,
        date: e.date.clone(),
        method: e.method,
        payer,
        total,
        lines,
        overrides: applied,
        rounding,
        counted: payer.is_some() && !parts.is_empty(),
        warnings,
    }
}

/// Fewest-payments settle-up: repeatedly match the largest debtor with the largest creditor.
pub fn settle(balances: &[(Id, i64)]) -> Vec<Transfer> {
    let mut debt: Vec<(Id, i64)> = balances
        .iter()
        .filter(|b| b.1 < 0)
        .map(|b| (b.0, -b.1))
        .collect();
    let mut cred: Vec<(Id, i64)> = balances.iter().filter(|b| b.1 > 0).copied().collect();
    let mut out = vec![];
    loop {
        debt.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        cred.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let (Some(d), Some(c)) = (debt.first_mut(), cred.first_mut()) else {
            break;
        };
        if d.1 == 0 || c.1 == 0 {
            break;
        }
        let amt = d.1.min(c.1);
        out.push(Transfer {
            from: d.0,
            to: c.0,
            amount: amt,
        });
        d.1 -= amt;
        c.1 -= amt;
        debt.retain(|x| x.1 > 0);
        cred.retain(|x| x.1 > 0);
    }
    out
}

pub fn compute(g: &Group) -> Report {
    let cur = g.currency;
    let mut warnings: Vec<String> = vec![];

    for o in &g.overrides {
        if o.description.trim().is_empty() {
            warnings.push(format!(
                "An override for {} has no description.",
                g.person_name(o.person)
            ));
        }
    }

    let group_excluded: Vec<Id> = g
        .overrides
        .iter()
        .filter(|o| {
            o.scope == OverrideScope::Group
                && o.kind == OverrideKind::Exclude
                && g.person(o.person).is_some()
        })
        .map(|o| o.person)
        .collect();

    let expenses: Vec<ExpenseResult> = g
        .expenses
        .iter()
        .map(|e| compute_expense(e, g, &group_excluded))
        .collect();

    let mut people: Vec<PersonSummary> = g
        .people
        .iter()
        .map(|p| PersonSummary {
            id: p.id,
            name: g.person_name(p.id),
            ..Default::default()
        })
        .collect();
    let mut total_spent = 0;
    for r in expenses.iter().filter(|r| r.counted) {
        total_spent += r.total;
        if let Some(pp) = people.iter_mut().find(|x| Some(x.id) == r.payer) {
            pp.paid += r.total;
        }
        for l in &r.lines {
            if let Some(pp) = people.iter_mut().find(|x| x.id == l.person) {
                pp.expense_share += l.amount;
            }
        }
    }

    // Who paid the most handles group-level rounding.
    let top_payer = people
        .iter()
        .max_by_key(|p| (p.paid, std::cmp::Reverse(p.id)))
        .filter(|p| p.paid > 0)
        .map(|p| p.id);

    // Group overrides.
    let mut totals: Vec<(Id, i64)> = people.iter().map(|p| (p.id, p.expense_share)).collect();
    let mut group_overrides = vec![];
    let mut group_rounding = vec![];
    for o in g
        .overrides
        .iter()
        .filter(|o| o.scope == OverrideScope::Group && g.person(o.person).is_some())
    {
        if o.kind == OverrideKind::Exclude {
            group_overrides.push(AppliedOverride {
                override_id: o.id,
                person: o.person,
                description: o.description.clone(),
                effect: "Pays nothing for any expense (excluded from all)".into(),
                before: 0,
                after: 0,
            });
            continue;
        }
        let locked: Vec<Id> = group_overrides
            .iter()
            .filter(|a: &&AppliedOverride| {
                g.overrides
                    .iter()
                    .any(|x| x.id == a.override_id && matches!(x.kind, OverrideKind::Fixed(_)))
            })
            .map(|a| a.person)
            .chain(group_excluded.iter().copied())
            .collect();
        match apply_one(&mut totals, o.person, &o.kind, &locked, top_payer) {
            Ok((before, after, r)) => {
                if let Some((p, a)) = r {
                    group_rounding.push(RoundingNote {
                        person: p,
                        amount: a,
                        context: format!("Whole group: re-splitting after override \"{}\" (goes to the top payer)", o.description),
                    });
                }
                group_overrides.push(AppliedOverride {
                    override_id: o.id,
                    person: o.person,
                    description: o.description.clone(),
                    effect: format!(
                        "{} in total (was {})",
                        describe_kind(&o.kind, cur),
                        cur.format(before)
                    ),
                    before,
                    after,
                });
            }
            Err(msg) => warnings.push(format!(
                "Group override \"{}\" could not be applied: {}.",
                o.description, msg
            )),
        }
    }
    for p in people.iter_mut() {
        p.owed = totals
            .iter()
            .find(|t| t.0 == p.id)
            .map(|t| t.1)
            .unwrap_or(0);
    }

    // Donations / sponsors.
    let mut total_contributions = 0;
    for c in &g.contributions {
        match c.received_by.filter(|id| g.person(*id).is_some()) {
            Some(holder) => {
                total_contributions += c.amount;
                if let Some(pp) = people.iter_mut().find(|p| p.id == holder) {
                    pp.holding += c.amount;
                }
            }
            None => warnings.push(format!(
                "Donation \"{}\" has no member holding the money, so it was not applied.",
                c.description
            )),
        }
    }
    if total_contributions != 0 {
        if total_contributions > total_spent {
            warnings.push(format!(
                "Donations ({}) are more than the total spent ({}); the surplus is paid back to members.",
                cur.format(total_contributions),
                cur.format(total_spent)
            ));
        }
        let weights: Vec<(Id, f64)> = people.iter().map(|p| (p.id, p.owed as f64)).collect();
        let (credits, r) = allocate(total_contributions, &weights, top_payer);
        if let Some((p, a)) = r {
            group_rounding.push(RoundingNote {
                person: p,
                amount: -a,
                context: "Donation credit did not divide evenly (extra credit to the top payer)"
                    .into(),
            });
        }
        for (id, a) in credits {
            if let Some(pp) = people.iter_mut().find(|p| p.id == id) {
                pp.credit = a;
            }
        }
    }

    for p in people.iter_mut() {
        p.net_owed = p.owed - p.credit;
        p.balance = p.paid - p.net_owed - p.holding;
    }
    let sum: i64 = people.iter().map(|p| p.balance).sum();
    if sum != 0 {
        warnings.push(format!(
            "Internal check: balances are off by {}.",
            cur.format(sum)
        ));
    }
    let transfers = settle(&people.iter().map(|p| (p.id, p.balance)).collect::<Vec<_>>());

    for r in &expenses {
        for w in &r.warnings {
            warnings.push(format!("{}: {}", r.description, w));
        }
    }

    Report {
        currency: cur,
        expenses,
        group_overrides,
        group_rounding,
        people,
        transfers,
        total_spent,
        total_contributions,
        warnings,
    }
}
