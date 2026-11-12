//! Saved data model. Everything here round-trips through the `.groupkeep` JSON file.

use crate::money::Currency;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Id = u32;

pub const FILE_EXTENSION: &str = "groupkeep";
pub const FILE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GroupType {
    #[default]
    Simple,
    Meal,
    DayTrip,
    Vacation,
    CommunityEvent,
    Other,
}

impl GroupType {
    pub const ALL: [GroupType; 6] = [
        GroupType::Simple,
        GroupType::Meal,
        GroupType::DayTrip,
        GroupType::Vacation,
        GroupType::CommunityEvent,
        GroupType::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GroupType::Simple => "Simple",
            GroupType::Meal => "Meal",
            GroupType::DayTrip => "Day Trip",
            GroupType::Vacation => "Vacation",
            GroupType::CommunityEvent => "Community Event",
            GroupType::Other => "All Others",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            GroupType::Simple => "One bill, split once.",
            GroupType::Meal => "Itemized bill with tax and tip.",
            GroupType::DayTrip => "Several expenses on one day.",
            GroupType::Vacation => "Multi-day trip; split lodging by nights attended.",
            GroupType::CommunityEvent => "Households as units, with donations and sponsors.",
            GroupType::Other => "Anything else: all methods available.",
        }
    }

    pub fn from_index(i: usize) -> GroupType {
        GroupType::ALL.get(i).copied().unwrap_or_default()
    }

    pub fn index(self) -> usize {
        GroupType::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    /// Division methods offered for this group type (all are valid in the engine).
    pub fn methods(self) -> Vec<Method> {
        use Method::*;
        match self {
            GroupType::Simple => vec![Equal, Shares, Percent, Exact],
            GroupType::Meal => vec![ByItem, Equal, Shares, Percent, Exact],
            GroupType::DayTrip => vec![Equal, Shares, Percent, Exact, ByItem],
            GroupType::Vacation => vec![ByAttendance, Equal, Shares, Percent, Exact, ByItem],
            GroupType::CommunityEvent => vec![Shares, Equal, Percent, Exact],
            GroupType::Other => Method::ALL.to_vec(),
        }
    }

    pub fn default_method(self) -> Method {
        self.methods()[0]
    }

    /// What one "person" row represents for this type.
    pub fn member_noun(self) -> &'static str {
        match self {
            GroupType::CommunityEvent => "Household",
            _ => "Person",
        }
    }

    /// Label for the per-member `shares` number.
    pub fn shares_label(self) -> &'static str {
        match self {
            GroupType::CommunityEvent => "Household size",
            _ => "Shares",
        }
    }

    pub fn uses_days(self) -> bool {
        matches!(self, GroupType::Vacation)
    }

    pub fn uses_contributions(self) -> bool {
        matches!(self, GroupType::CommunityEvent | GroupType::Other)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Method {
    /// Same amount for every participant.
    Equal,
    /// Weighted by each participant's shares (adult 1, child 0.5, household size…).
    Shares,
    /// Each participant gets a percentage.
    Percent,
    /// Exact amount per participant.
    Exact,
    /// Itemized: each item split among those who had it; tax and tip proportional to subtotal.
    ByItem,
    /// Split day-by-day among those present on each covered day.
    ByAttendance,
}

impl Method {
    pub const ALL: [Method; 6] = [
        Method::Equal,
        Method::Shares,
        Method::Percent,
        Method::Exact,
        Method::ByItem,
        Method::ByAttendance,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Method::Equal => "Equal",
            Method::Shares => "Shares",
            Method::Percent => "Percent",
            Method::Exact => "Exact amounts",
            Method::ByItem => "By item",
            Method::ByAttendance => "By attendance",
        }
    }

    pub fn explain(self) -> &'static str {
        match self {
            Method::Equal => "Everyone checked pays the same.",
            Method::Shares => "Split in proportion to each person's shares.",
            Method::Percent => "Each person pays a set percentage (should total 100%).",
            Method::Exact => "Each person pays an exact amount (should total the expense).",
            Method::ByItem => "Each item is split among the people who had it; tax and tip follow each person's subtotal.",
            Method::ByAttendance => "The cost is spread over the covered days and split among whoever was there each day.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub id: Id,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Default weight for the Shares method (household size for community events).
    #[serde(default = "one")]
    pub shares: f64,
    /// Vacation: one flag per trip day.
    #[serde(default)]
    pub attendance: Vec<bool>,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub description: String,
    pub amount: i64,
    /// Who had this item. Empty = shared by every participant.
    #[serde(default)]
    pub shared_by: Vec<Id>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum Tip {
    #[default]
    None,
    /// Percent of the item subtotal (pre-tax).
    Percent(f64),
    Amount(i64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expense {
    pub id: Id,
    pub description: String,
    #[serde(default)]
    pub date: String,
    pub paid_by: Option<Id>,
    /// Total for every method except ByItem (whose total is items + tax + tip).
    #[serde(default)]
    pub amount: i64,
    pub method: Method,
    /// Who shares this expense.
    pub participants: Vec<Id>,
    /// Shares method: per-expense weights (falls back to the person's default shares).
    #[serde(default)]
    pub shares: BTreeMap<Id, f64>,
    #[serde(default)]
    pub percents: BTreeMap<Id, f64>,
    #[serde(default)]
    pub exact: BTreeMap<Id, i64>,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub tax: i64,
    #[serde(default)]
    pub tip: Tip,
    /// ByAttendance: which trip days this expense covers (empty = all days).
    #[serde(default)]
    pub days: Vec<bool>,
}

impl Expense {
    /// The amount this expense actually costs.
    pub fn total(&self) -> i64 {
        match self.method {
            Method::ByItem => self.item_subtotal() + self.tax + self.tip_amount(),
            _ => self.amount,
        }
    }

    pub fn item_subtotal(&self) -> i64 {
        self.items.iter().map(|i| i.amount).sum()
    }

    pub fn tip_amount(&self) -> i64 {
        match self.tip {
            Tip::None => 0,
            Tip::Percent(p) => (self.item_subtotal() as f64 * p / 100.0).round() as i64,
            Tip::Amount(a) => a,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverrideScope {
    /// Applies to one expense.
    Expense(Id),
    /// Applies to the person's total across the whole group.
    Group,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OverrideKind {
    /// The person pays exactly this amount.
    Fixed(i64),
    /// Add (or subtract, if negative) this amount.
    Adjust(i64),
    /// Change the person's share by this percent (-50 = half off).
    AdjustPercent(f64),
    /// The person pays nothing.
    Exclude,
}

impl OverrideKind {
    pub const LABELS: [&'static str; 4] = [
        "Pays exactly",
        "Add / subtract amount",
        "Change by percent",
        "Pays nothing (exclude)",
    ];

    pub fn index(&self) -> usize {
        match self {
            OverrideKind::Fixed(_) => 0,
            OverrideKind::Adjust(_) => 1,
            OverrideKind::AdjustPercent(_) => 2,
            OverrideKind::Exclude => 3,
        }
    }
}

/// A manual change to the computed split. The description is required and is shown everywhere
/// the override has an effect, so nobody wonders why a number looks off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Override {
    pub id: Id,
    pub description: String,
    pub person: Id,
    pub scope: OverrideScope,
    pub kind: OverrideKind,
}

/// Money from outside the group (donation, sponsor) that lowers what members owe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contribution {
    pub id: Id,
    pub description: String,
    #[serde(default)]
    pub from: String,
    pub amount: i64,
    /// Member who is holding the money.
    pub received_by: Option<Id>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    #[serde(default = "version")]
    pub format_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub group_type: GroupType,
    pub currency: Currency,
    /// Vacation: first day (YYYY-MM-DD) and number of days.
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub num_days: u32,
    pub people: Vec<Person>,
    pub expenses: Vec<Expense>,
    #[serde(default)]
    pub overrides: Vec<Override>,
    #[serde(default)]
    pub contributions: Vec<Contribution>,
    #[serde(default)]
    pub next_id: Id,
}

fn version() -> u32 {
    FILE_FORMAT_VERSION
}

impl Default for Group {
    fn default() -> Self {
        Group {
            format_version: FILE_FORMAT_VERSION,
            name: "New group".into(),
            description: String::new(),
            group_type: GroupType::Simple,
            currency: Currency::USD,
            start_date: chrono::Local::now()
                .date_naive()
                .format("%Y-%m-%d")
                .to_string(),
            num_days: 3,
            people: vec![],
            expenses: vec![],
            overrides: vec![],
            contributions: vec![],
            next_id: 1,
        }
    }
}

impl Group {
    pub fn new_id(&mut self) -> Id {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        id
    }

    pub fn person(&self, id: Id) -> Option<&Person> {
        self.people.iter().find(|p| p.id == id)
    }

    pub fn person_name(&self, id: Id) -> String {
        self.person(id)
            .map(|p| {
                if p.name.trim().is_empty() {
                    "(unnamed)".to_string()
                } else {
                    p.name.clone()
                }
            })
            .unwrap_or_else(|| "(removed)".into())
    }

    pub fn expense(&self, id: Id) -> Option<&Expense> {
        self.expenses.iter().find(|e| e.id == id)
    }

    pub fn expense_mut(&mut self, id: Id) -> Option<&mut Expense> {
        self.expenses.iter_mut().find(|e| e.id == id)
    }

    pub fn add_person(&mut self, name: &str) -> Id {
        let id = self.new_id();
        let days = self.num_days as usize;
        self.people.push(Person {
            id,
            name: name.into(),
            description: String::new(),
            shares: 1.0,
            attendance: vec![true; days],
        });
        // New people join every existing expense by default.
        for e in &mut self.expenses {
            e.participants.push(id);
        }
        id
    }

    pub fn remove_person(&mut self, id: Id) {
        self.people.retain(|p| p.id != id);
        for e in &mut self.expenses {
            e.participants.retain(|p| *p != id);
            e.shares.remove(&id);
            e.percents.remove(&id);
            e.exact.remove(&id);
            for it in &mut e.items {
                it.shared_by.retain(|p| *p != id);
            }
            if e.paid_by == Some(id) {
                e.paid_by = None;
            }
        }
        self.overrides.retain(|o| o.person != id);
        for c in &mut self.contributions {
            if c.received_by == Some(id) {
                c.received_by = None;
            }
        }
    }

    pub fn add_expense(&mut self) -> Id {
        let id = self.new_id();
        let method = self.group_type.default_method();
        let date = self.start_date.clone();
        let n = self.expenses.len() + 1;
        let description = match self.group_type {
            GroupType::Meal => "Dinner".to_string(),
            GroupType::Vacation if n == 1 => "Lodging".to_string(),
            _ => format!("Expense {}", n),
        };
        self.expenses.push(Expense {
            id,
            description,
            date,
            paid_by: self.people.first().map(|p| p.id),
            amount: 0,
            method,
            participants: self.people.iter().map(|p| p.id).collect(),
            shares: BTreeMap::new(),
            percents: BTreeMap::new(),
            exact: BTreeMap::new(),
            items: vec![],
            tax: 0,
            tip: if method == Method::ByItem {
                Tip::Percent(18.0)
            } else {
                Tip::None
            },
            days: vec![],
        });
        id
    }

    pub fn remove_expense(&mut self, id: Id) {
        self.expenses.retain(|e| e.id != id);
        self.overrides
            .retain(|o| o.scope != OverrideScope::Expense(id));
    }

    /// Change currency. Amounts keep their face value ("10.00" stays "10"); this is not an
    /// exchange-rate conversion, only a change of decimal places when needed (e.g. USD → JPY).
    pub fn set_currency(&mut self, c: Currency) {
        let from = self.currency.decimals() as i32;
        let to = c.decimals() as i32;
        self.currency = c;
        if from == to {
            return;
        }
        let conv = |v: i64| -> i64 {
            if to > from {
                v * 10i64.pow((to - from) as u32)
            } else {
                let d = 10i64.pow((from - to) as u32);
                (v as f64 / d as f64).round() as i64
            }
        };
        for e in &mut self.expenses {
            e.amount = conv(e.amount);
            e.tax = conv(e.tax);
            for v in e.exact.values_mut() {
                *v = conv(*v);
            }
            for it in &mut e.items {
                it.amount = conv(it.amount);
            }
            if let Tip::Amount(a) = e.tip {
                e.tip = Tip::Amount(conv(a));
            }
        }
        for o in &mut self.overrides {
            o.kind = match o.kind {
                OverrideKind::Fixed(a) => OverrideKind::Fixed(conv(a)),
                OverrideKind::Adjust(a) => OverrideKind::Adjust(conv(a)),
                k => k,
            };
        }
        for c in &mut self.contributions {
            c.amount = conv(c.amount);
        }
    }

    /// Keep per-person attendance vectors the same length as the trip.
    pub fn set_num_days(&mut self, n: u32) {
        let n = n.clamp(1, 60);
        self.num_days = n;
        for p in &mut self.people {
            p.attendance.resize(n as usize, true);
        }
        for e in &mut self.expenses {
            if !e.days.is_empty() {
                e.days.resize(n as usize, true);
            }
        }
    }

    /// "Thu Nov 5" style labels for each trip day; falls back to "Day 1".
    pub fn day_labels(&self) -> Vec<String> {
        let start = chrono::NaiveDate::parse_from_str(self.start_date.trim(), "%Y-%m-%d").ok();
        (0..self.num_days.max(1))
            .map(|i| match start {
                Some(d) => (d + chrono::Duration::days(i as i64))
                    .format("%a %b %-d")
                    .to_string(),
                None => format!("Day {}", i + 1),
            })
            .collect()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("group serializes")
    }

    pub fn from_json(s: &str) -> Result<Group, String> {
        let mut g: Group =
            serde_json::from_str(s).map_err(|e| format!("Not a valid GroupKeep file: {e}"))?;
        if g.format_version > FILE_FORMAT_VERSION {
            return Err(format!(
                "This file was made by a newer GroupKeep (format {}). Please update.",
                g.format_version
            ));
        }
        // Repair ids if an older/hand-edited file lacks next_id.
        let max_id = g
            .people
            .iter()
            .map(|p| p.id)
            .chain(g.expenses.iter().map(|e| e.id))
            .chain(g.overrides.iter().map(|o| o.id))
            .chain(g.contributions.iter().map(|c| c.id))
            .max()
            .unwrap_or(0);
        g.next_id = g.next_id.max(max_id + 1);
        let n = g.num_days;
        if g.group_type.uses_days() {
            g.set_num_days(n.max(1));
        }
        Ok(g)
    }
}
