//! Drives the UI callbacks headlessly like a user would, and checks the resulting data.
//! cargo run -p groupkeep --example flow

use groupkeep_app::{controller::Controller, AppWindow};
use groupkeep_core::{calc, Currency, Method, OverrideKind, OverrideScope};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, Model};
use std::rc::Rc;

struct Headless(Rc<MinimalSoftwareWindow>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn main() {
    let win = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(win.clone()))).unwrap();
    let ui = AppWindow::new().unwrap();
    let ctl = Controller::setup(&ui);
    ui.show().unwrap();

    // People
    for n in ["Ann", "Bob", "Cy"] {
        ui.invoke_add_person(n.into());
    }
    assert_eq!(ui.get_people().row_count(), 3);
    let ids: Vec<u32> = ctl.group().people.iter().map(|p| p.id).collect();
    ui.invoke_person_shares_edited(ids[2] as i32, "0.5".into());
    assert_eq!(ctl.group().people[2].shares, 0.5);

    // Group name
    ui.set_group_name("Test trip".into());
    ui.invoke_group_changed();
    assert_eq!(ctl.group().name, "Test trip");
    assert!(ui.get_dirty());

    // Expense: $100 equal, paid by Bob
    ui.invoke_add_expense();
    assert!(ui.get_editing_expense());
    ui.set_edit_amount("100".into());
    ui.set_edit_payer_index(1);
    ui.set_edit_description("Gas".into());
    ui.invoke_expense_field_changed();
    let g = ctl.group();
    let e = &g.expenses[0];
    assert_eq!(
        (e.amount, e.paid_by, e.description.as_str()),
        (10000, Some(ids[1]), "Gas")
    );
    assert_eq!(ui.get_edit_total().as_str(), "$100.00");
    let row = ui.get_edit_shares().row_data(1).unwrap();
    assert_eq!(row.amount.as_str(), "$33.34", "payer gets the extra penny");

    // Switch to Shares (index 1 of Simple's methods: Equal, Shares, Percent, Exact)
    ui.invoke_method_selected(1);
    assert_eq!(ctl.group().expenses[0].method, Method::Shares);
    assert_eq!(ui.get_edit_value_label().as_str(), "shares");
    let amounts: Vec<String> = (0..3)
        .map(|i| ui.get_edit_shares().row_data(i).unwrap().amount.to_string())
        .collect();
    assert_eq!(amounts, vec!["$40.00", "$40.00", "$20.00"]);

    // Percent with a bad total shows a red footer
    ui.invoke_method_selected(2);
    ui.invoke_share_value_edited(ids[0] as i32, "50".into());
    ui.invoke_share_value_edited(ids[1] as i32, "30".into());
    assert!(ui.get_edit_shares_footer_bad());
    ui.invoke_share_value_edited(ids[2] as i32, "20".into());
    assert!(!ui.get_edit_shares_footer_bad());

    // Remove Cy from this expense
    ui.invoke_participant_toggled(ids[2] as i32);
    assert_eq!(ctl.group().expenses[0].participants.len(), 2);
    ui.invoke_participant_toggled(ids[2] as i32);

    // Override from inside the expense: Ann pays exactly $10
    ui.invoke_new_override(true);
    assert!(ui.get_ov_open());
    assert_eq!(ui.get_ov_scope_index(), 1, "pre-scoped to this expense");
    ui.invoke_save_override();
    assert!(
        ui.get_ov_error().contains("describe"),
        "description required"
    );
    ui.set_ov_description("Ann drove".into());
    ui.invoke_save_override();
    assert!(ui.get_ov_error().contains("who"), "person required");
    ui.set_ov_person_index(0);
    ui.set_ov_value("10".into());
    ui.invoke_save_override();
    assert!(!ui.get_ov_open());
    let g = ctl.group();
    assert_eq!(g.overrides.len(), 1);
    assert_eq!(g.overrides[0].kind, OverrideKind::Fixed(1000));
    assert_eq!(
        g.overrides[0].scope,
        OverrideScope::Expense(g.expenses[0].id)
    );
    assert_eq!(ui.get_edit_overrides().row_count(), 1);
    let r = calc::compute(&g);
    assert_eq!(r.expenses[0].lines[0].amount, 1000);
    assert_eq!(r.people.iter().map(|p| p.balance).sum::<i64>(), 0);
    ui.invoke_close_expense();
    assert_eq!(ui.get_overrides().row_count(), 1);
    assert_eq!(ui.get_expenses().row_data(0).unwrap().override_count, 1);

    // Meal-style itemized expense on "All Others"
    ui.invoke_group_type_selected(5);
    ui.invoke_add_expense();
    let methods: Vec<String> = ui
        .get_edit_methods()
        .iter()
        .map(|m| m.title.to_string())
        .collect();
    let by_item = methods.iter().position(|m| m == "By item").unwrap();
    ui.invoke_method_selected(by_item as i32);
    assert_eq!(ui.get_edit_method_key().as_str(), "item");
    ui.invoke_item_amount_edited(0, "30".into());
    ui.invoke_item_who_toggled(0, ids[0] as i32);
    ui.invoke_add_item();
    ui.invoke_item_amount_edited(1, "15".into());
    ui.set_edit_tax("4.50".into());
    ui.set_edit_tip_mode(1);
    ui.set_edit_tip_value(String::new().into());
    ui.invoke_expense_field_changed();
    assert_eq!(ui.get_edit_tip_value().as_str(), "18", "default tip");
    assert_eq!(ui.get_edit_total().as_str(), "$57.60"); // 45 + 4.50 + 8.10
    ui.invoke_close_expense();

    // Donation
    ui.invoke_add_contribution();
    let cid = ctl.group().contributions[0].id as i32;
    ui.invoke_contribution_edited(cid, "Sponsor".into(), "Shop".into(), "20".into());
    assert_eq!(ctl.group().contributions[0].amount, 2000);

    // Currency change keeps face value
    ui.invoke_currency_selected(Currency::JPY.index() as i32);
    assert_eq!(ctl.group().expenses[0].amount, 100);
    ui.invoke_currency_selected(Currency::USD.index() as i32);
    assert_eq!(ctl.group().expenses[0].amount, 10000);

    // Save/open round trip through a real file (no dialog).
    let path = std::env::temp_dir().join("flow-test.groupkeep");
    std::fs::write(&path, ctl.group().to_json()).unwrap();
    let before = ctl.group();
    ui.invoke_new_group();
    assert!(ui.get_confirm_open(), "asks before discarding");
    ui.invoke_confirm_yes();
    assert_eq!(ctl.group().people.len(), 0);
    ctl.open_path(path.clone());
    assert_eq!(ctl.group(), before);
    assert!(!ui.get_dirty());

    // Delete expense with confirm
    ui.invoke_open_expense(before.expenses[0].id as i32);
    ui.invoke_delete_expense();
    ui.invoke_confirm_yes();
    assert_eq!(ctl.group().expenses.len(), 1);
    assert!(ctl.group().overrides.is_empty(), "its overrides go too");

    // Remove a person
    ui.invoke_remove_person(ids[1] as i32);
    assert_eq!(ctl.group().people.len(), 2);

    let r = calc::compute(&ctl.group());
    assert_eq!(r.people.iter().map(|p| p.balance).sum::<i64>(), 0);
    println!(
        "flow OK: {} warnings after edits: {:?}",
        r.warnings.len(),
        r.warnings
    );
}
