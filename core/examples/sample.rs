//! cargo run -p groupkeep-core --example sample -- out_dir
use groupkeep_core::*;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let g = samples::vacation();
    let r = compute(&g);
    for p in &r.people {
        println!(
            "{:<6} paid {:>10} owes {:>10} balance {:>10}",
            p.name,
            g.currency.format(p.paid),
            g.currency.format(p.owed),
            g.currency.format(p.balance)
        );
    }
    for t in &r.transfers {
        println!(
            "{} pays {} {}",
            g.person_name(t.from),
            g.person_name(t.to),
            g.currency.format(t.amount)
        );
    }
    for w in &r.warnings {
        println!("WARN {w}");
    }
    std::fs::write(format!("{dir}/Lake House Weekend.groupkeep"), g.to_json()).unwrap();
    std::fs::write(
        format!("{dir}/Lake House Weekend.pdf"),
        pdf::render(&g, &r).unwrap(),
    )
    .unwrap();
}
