//! Ready-made example groups (used for screenshots, tests and a first-run demo).
use crate::model::*;

pub fn vacation() -> Group {
    let mut g = Group {
        name: "Lake House Weekend".into(),
        description: "Four days at the lake. Lodging is split by nights attended.".into(),
        group_type: GroupType::Vacation,
        start_date: "2026-11-05".into(),
        ..Group::default()
    };
    g.set_num_days(4);
    let ann = g.add_person("Ann");
    let bob = g.add_person("Bob");
    let cy = g.add_person("Cy");
    let dee = g.add_person("Dee");
    g.people[1].description = "Arrived Saturday".into();
    g.people[1].attendance = vec![false, false, true, true];
    g.people[3].description = "Kid; counts as a half share".into();
    g.people[3].shares = 0.5;

    let lodging = g.add_expense();
    {
        let e = g.expense_mut(lodging).unwrap();
        e.description = "Cabin rental".into();
        e.amount = 80000;
        e.paid_by = Some(ann);
    }
    let groceries = g.add_expense();
    {
        let e = g.expense_mut(groceries).unwrap();
        e.description = "Groceries".into();
        e.method = Method::Shares;
        e.amount = 21050;
        e.paid_by = Some(cy);
    }
    let dinner = g.add_expense();
    {
        let e = g.expense_mut(dinner).unwrap();
        e.description = "Dinner at the Dockside".into();
        e.date = "2026-11-07".into();
        e.method = Method::ByItem;
        e.paid_by = Some(bob);
        e.items = vec![
            Item {
                description: "Walleye".into(),
                amount: 2800,
                shared_by: vec![ann],
            },
            Item {
                description: "Burger".into(),
                amount: 1650,
                shared_by: vec![bob],
            },
            Item {
                description: "Pasta".into(),
                amount: 1900,
                shared_by: vec![cy],
            },
            Item {
                description: "Kids meal".into(),
                amount: 899,
                shared_by: vec![dee],
            },
            Item {
                description: "Nachos".into(),
                amount: 1400,
                shared_by: vec![],
            },
        ];
        e.tax = 694;
        e.tip = Tip::Percent(20.0);
    }
    let id = g.new_id();
    g.overrides.push(Override {
        id,
        description: "Dee slept on the couch".into(),
        person: dee,
        scope: OverrideScope::Expense(lodging),
        kind: OverrideKind::AdjustPercent(-50.0),
    });
    let id = g.new_id();
    g.overrides.push(Override {
        id,
        description: "Bob brought the boat gas, covers $25 less overall".into(),
        person: bob,
        scope: OverrideScope::Group,
        kind: OverrideKind::Adjust(-2500),
    });

    g
}

pub fn meal() -> Group {
    let mut g = Group {
        name: "Friday dinner".into(),
        group_type: GroupType::Meal,
        ..Group::default()
    };
    let a = g.add_person("Maria");
    let b = g.add_person("Tom");
    let c = g.add_person("Priya");
    let e = g.add_expense();
    let x = g.expense_mut(e).unwrap();
    x.description = "Rosa's Trattoria".into();
    x.paid_by = Some(a);
    x.items = vec![
        Item {
            description: "Margherita".into(),
            amount: 1600,
            shared_by: vec![a, b],
        },
        Item {
            description: "Lasagna".into(),
            amount: 2100,
            shared_by: vec![c],
        },
        Item {
            description: "Tiramisu".into(),
            amount: 900,
            shared_by: vec![],
        },
    ];
    x.tax = 322;
    x.tip = Tip::Percent(20.0);
    g
}

pub fn community() -> Group {
    let mut g = Group {
        name: "Fall Festival".into(),
        description: "Church fall festival costs, split by household size.".into(),
        group_type: GroupType::CommunityEvent,
        ..Group::default()
    };
    let a = g.add_person("Rivera family");
    let b = g.add_person("Lopez family");
    let c = g.add_person("Chen family");
    g.people[0].shares = 4.0;
    g.people[1].shares = 5.0;
    g.people[2].shares = 2.0;
    let e = g.add_expense();
    let x = g.expense_mut(e).unwrap();
    x.description = "Tent and tables".into();
    x.amount = 45000;
    x.paid_by = Some(a);
    let e = g.add_expense();
    let x = g.expense_mut(e).unwrap();
    x.description = "Food and drinks".into();
    x.amount = 31275;
    x.paid_by = Some(b);
    let id = g.new_id();
    g.contributions.push(Contribution {
        id,
        description: "Sponsor banner".into(),
        from: "Main St Hardware".into(),
        amount: 10000,
        received_by: Some(c),
    });
    g
}
