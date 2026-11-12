use crate::calc::*;
use crate::model::*;

fn group(names: &[&str]) -> (Group, Vec<Id>) {
    let mut g = Group::default();
    let ids = names.iter().map(|n| g.add_person(n)).collect();
    (g, ids)
}

fn share(r: &Report, exp: usize, p: Id) -> i64 {
    r.expenses[exp]
        .lines
        .iter()
        .find(|l| l.person == p)
        .map(|l| l.amount)
        .unwrap_or(0)
}

fn assert_balanced(r: &Report) {
    assert_eq!(
        r.people.iter().map(|p| p.balance).sum::<i64>(),
        0,
        "balances must sum to zero"
    );
    for e in r.expenses.iter().filter(|e| e.counted) {
        assert_eq!(
            e.lines.iter().map(|l| l.amount).sum::<i64>(),
            e.total,
            "{} shares must sum to total",
            e.description
        );
    }
}

#[test]
fn equal_split_penny_goes_to_payer() {
    let (mut g, p) = group(&["Ann", "Bob", "Cy"]);
    let e = g.add_expense();
    g.expense_mut(e).unwrap().amount = 10000; // $100.00 / 3
    g.expense_mut(e).unwrap().paid_by = Some(p[1]);
    let r = compute(&g);
    assert_eq!(share(&r, 0, p[0]), 3333);
    assert_eq!(share(&r, 0, p[1]), 3334);
    assert_eq!(share(&r, 0, p[2]), 3333);
    let rn = r.all_rounding();
    assert_eq!(rn.len(), 1);
    assert_eq!(rn[0].person, p[1]);
    assert_eq!(rn[0].amount, 1);
    assert_balanced(&r);
    assert_eq!(r.transfers.len(), 2);
    assert!(r.transfers.iter().all(|t| t.to == p[1] && t.amount == 3333));
}

#[test]
fn shares_method() {
    let (mut g, p) = group(&["Adult", "Child"]);
    g.people[1].shares = 0.5;
    let e = g.add_expense();
    let ex = g.expense_mut(e).unwrap();
    ex.method = Method::Shares;
    ex.amount = 9000;
    let r = compute(&g);
    assert_eq!(share(&r, 0, p[0]), 6000);
    assert_eq!(share(&r, 0, p[1]), 3000);
    assert_balanced(&r);
}

#[test]
fn percent_and_exact_warnings() {
    let (mut g, p) = group(&["A", "B"]);
    let e = g.add_expense();
    let ex = g.expense_mut(e).unwrap();
    ex.method = Method::Percent;
    ex.amount = 1000;
    ex.percents.insert(p[0], 70.0);
    ex.percents.insert(p[1], 30.0);
    let r = compute(&g);
    assert_eq!(share(&r, 0, p[0]), 700);
    assert!(r.warnings.is_empty());

    let ex = g.expense_mut(e).unwrap();
    ex.method = Method::Exact;
    ex.exact.insert(p[0], 600);
    ex.exact.insert(p[1], 300);
    let r = compute(&g);
    assert_eq!(share(&r, 0, p[0]), 650);
    assert_eq!(share(&r, 0, p[1]), 350);
    assert_eq!(r.warnings.len(), 1);
    assert_balanced(&r);
}

#[test]
fn meal_by_item_with_tax_and_tip() {
    let (mut g, p) = group(&["Ann", "Bob"]);
    g.group_type = GroupType::Meal;
    let e = g.add_expense();
    let ex = g.expense_mut(e).unwrap();
    assert_eq!(ex.method, Method::ByItem);
    ex.items = vec![
        Item {
            description: "Steak".into(),
            amount: 3000,
            shared_by: vec![p[0]],
        },
        Item {
            description: "Salad".into(),
            amount: 1000,
            shared_by: vec![p[1]],
        },
        Item {
            description: "Wine".into(),
            amount: 2000,
            shared_by: vec![],
        }, // shared
    ];
    ex.tax = 600; // 10% of 6000
    ex.tip = Tip::Percent(20.0); // 1200
    let r = compute(&g);
    assert_eq!(r.expenses[0].total, 7800);
    // Ann subtotal 4000, Bob 2000 → extras 1800 split 2:1
    assert_eq!(share(&r, 0, p[0]), 5200);
    assert_eq!(share(&r, 0, p[1]), 2600);
    assert_balanced(&r);
}

#[test]
fn vacation_by_attendance() {
    let (mut g, p) = group(&["Ann", "Bob"]);
    g.group_type = GroupType::Vacation;
    g.set_num_days(4);
    g.people[1].attendance = vec![false, false, true, true];
    let e = g.add_expense();
    let ex = g.expense_mut(e).unwrap();
    assert_eq!(ex.method, Method::ByAttendance);
    ex.amount = 40000; // $100/day
    let r = compute(&g);
    // Days 1-2 Ann alone (20000), days 3-4 split (10000 each)
    assert_eq!(share(&r, 0, p[0]), 30000);
    assert_eq!(share(&r, 0, p[1]), 10000);
    assert_balanced(&r);
}

#[test]
fn fixed_override_resplits_others() {
    let (mut g, p) = group(&["Ann", "Bob", "Cy"]);
    let e = g.add_expense();
    g.expense_mut(e).unwrap().amount = 9000;
    let id = g.new_id();
    g.overrides.push(Override {
        id,
        description: "Bob only had an appetizer".into(),
        person: p[1],
        scope: OverrideScope::Expense(e),
        kind: OverrideKind::Fixed(1000),
    });
    let r = compute(&g);
    assert_eq!(share(&r, 0, p[1]), 1000);
    assert_eq!(share(&r, 0, p[0]), 4000);
    assert_eq!(share(&r, 0, p[2]), 4000);
    assert_eq!(r.expenses[0].overrides.len(), 1);
    assert!(r.expenses[0].overrides[0].effect.contains("was $30.00"));
    assert_balanced(&r);
}

#[test]
fn exclude_and_percent_override() {
    let (mut g, p) = group(&["Ann", "Bob", "Cy", "Dee"]);
    let e = g.add_expense();
    g.expense_mut(e).unwrap().amount = 12000;
    let id1 = g.new_id();
    let id2 = g.new_id();
    g.overrides.push(Override {
        id: id1,
        description: "Dee was not there".into(),
        person: p[3],
        scope: OverrideScope::Expense(e),
        kind: OverrideKind::Exclude,
    });
    g.overrides.push(Override {
        id: id2,
        description: "Cy birthday discount".into(),
        person: p[2],
        scope: OverrideScope::Expense(e),
        kind: OverrideKind::AdjustPercent(-50.0),
    });
    let r = compute(&g);
    assert_eq!(share(&r, 0, p[3]), 0);
    assert_eq!(share(&r, 0, p[2]), 2000); // 4000 halved
    assert_eq!(share(&r, 0, p[0]) + share(&r, 0, p[1]), 10000);
    assert_balanced(&r);
}

#[test]
fn group_override_and_donation() {
    let (mut g, p) = group(&["Smiths", "Joneses"]);
    g.group_type = GroupType::CommunityEvent;
    g.people[0].shares = 4.0;
    g.people[1].shares = 2.0;
    let e = g.add_expense();
    let ex = g.expense_mut(e).unwrap();
    assert_eq!(ex.method, Method::Shares);
    ex.amount = 60000;
    ex.paid_by = Some(p[0]);
    let cid = g.new_id();
    g.contributions.push(Contribution {
        id: cid,
        description: "Hardware store sponsor".into(),
        from: "Main St Hardware".into(),
        amount: 6000,
        received_by: Some(p[1]),
    });
    let r = compute(&g);
    let s = r.person(p[0]).unwrap();
    let j = r.person(p[1]).unwrap();
    assert_eq!(s.owed, 40000);
    assert_eq!(s.credit, 4000);
    assert_eq!(j.credit, 2000);
    assert_eq!(j.holding, 6000);
    assert_balanced(&r);
    // Joneses owe 18000 net and hold 6000 of sponsor cash → pay Smiths 24000.
    assert_eq!(
        r.transfers,
        vec![Transfer {
            from: p[1],
            to: p[0],
            amount: 24000
        }]
    );

    let oid = g.new_id();
    g.overrides.push(Override {
        id: oid,
        description: "Joneses cover an extra $50 for the tent".into(),
        person: p[1],
        scope: OverrideScope::Group,
        kind: OverrideKind::Adjust(5000),
    });
    let r = compute(&g);
    assert_eq!(r.person(p[1]).unwrap().owed, 25000);
    assert_eq!(r.person(p[0]).unwrap().owed, 35000);
    assert_balanced(&r);
}

#[test]
fn removing_person_cleans_up() {
    let (mut g, p) = group(&["A", "B"]);
    let e = g.add_expense();
    g.expense_mut(e).unwrap().paid_by = Some(p[1]);
    g.remove_person(p[1]);
    assert_eq!(g.expense(e).unwrap().paid_by, None);
    assert_eq!(g.expense(e).unwrap().participants, vec![p[0]]);
}

#[test]
fn json_round_trip() {
    let (mut g, p) = group(&["A", "B"]);
    let e = g.add_expense();
    let id = g.new_id();
    g.overrides.push(Override {
        id,
        description: "x".into(),
        person: p[0],
        scope: OverrideScope::Expense(e),
        kind: OverrideKind::Adjust(-100),
    });
    let s = g.to_json();
    let back = Group::from_json(&s).unwrap();
    assert_eq!(back, g);
}

#[test]
fn settle_minimizes() {
    let t = settle(&[(1, -500), (2, -500), (3, 1000)]);
    assert_eq!(t.len(), 2);
    let t = settle(&[(1, -300), (2, 100), (3, 200)]);
    assert_eq!(t.len(), 2);
}

/// upgrade-resilience-1 UR-10 (GroupKeep L11): a 30.00 expense, three people, one fixed at
/// 40.00 is refused; two fixed shares of 20.00 give the third 0.00 and the notice.
#[test]
fn ur10_overrides_larger_than_the_total() {
    let (mut g, p) = group(&["Ann", "Bob", "Cy"]);
    let e = g.add_expense();
    g.expense_mut(e).unwrap().amount = 3000;
    g.expense_mut(e).unwrap().paid_by = Some(p[0]);
    let fixed = |g: &mut Group, person: Id, amount: i64| Override {
        id: g.new_id(),
        description: "fixed".into(),
        person,
        scope: OverrideScope::Expense(e),
        kind: OverrideKind::Fixed(amount),
    };
    let too_big = fixed(&mut g, p[0], 4000);
    assert_eq!(
        check_override(&g, &too_big),
        Err(SHARE_TOO_LARGE.to_string())
    );
    assert_eq!(
        SHARE_TOO_LARGE,
        "This share is more than the whole expense."
    );
    let whole = fixed(&mut g, p[0], 3000);
    assert_eq!(check_override(&g, &whole), Ok(()));

    let a = fixed(&mut g, p[0], 2000);
    let b = fixed(&mut g, p[1], 2000);
    assert_eq!(check_override(&g, &a), Ok(()));
    g.overrides.push(a);
    g.overrides.push(b);
    let r = compute(&g);
    assert_eq!(share(&r, 0, p[0]), 2000);
    assert_eq!(share(&r, 0, p[1]), 2000);
    assert_eq!(share(&r, 0, p[2]), 0, "never negative");
    assert!(r.expenses[0].lines.iter().all(|l| l.amount >= 0));
    assert!(
        r.expenses[0]
            .warnings
            .iter()
            .any(|w| w == SHARES_OVER_TOTAL),
        "{:?}",
        r.expenses[0].warnings
    );

    // Fixed again: one fixed share of 20.00 leaves 5.00 each to the others, and no notice.
    g.overrides.pop();
    let r = compute(&g);
    assert_eq!(share(&r, 0, p[1]), 500);
    assert_eq!(share(&r, 0, p[2]), 500);
    assert!(!r.expenses[0]
        .warnings
        .iter()
        .any(|w| w == SHARES_OVER_TOTAL));
    assert_balanced(&r);
}
