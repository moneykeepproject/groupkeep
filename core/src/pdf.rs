//! PDF report (US Letter) using printpdf's built-in Helvetica, so no font files are needed.

use crate::calc::{describe_override, trim_num, Report};
use crate::model::*;
use printpdf::*;

const PAGE_W: f32 = 215.9;
const PAGE_H: f32 = 279.4;
const MARGIN: f32 = 16.0;
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN;

/// Helvetica advance widths (1/1000 em) for ASCII 32..=126, from the standard AFM.
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

fn text_width_mm(s: &str, size: f32, bold: bool) -> f32 {
    let units: u32 = s
        .chars()
        .map(|c| {
            let c = c as u32;
            if (32..=126).contains(&c) {
                HELV[(c - 32) as usize] as u32
            } else {
                556
            }
        })
        .sum();
    let w = units as f32 / 1000.0 * size * 0.352_778; // pt → mm
    if bold {
        w * 1.06
    } else {
        w
    }
}

/// Replace characters WinAnsi cannot show.
fn clean(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{2013}' | '\u{2014}' => '-',
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201C}' | '\u{201D}' => '"',
            c if (c as u32) < 256 || "€£¥•…".contains(c) => c,
            _ => '?',
        })
        .collect()
}

fn wrap(s: &str, size: f32, bold: bool, width: f32) -> Vec<String> {
    let mut lines = vec![];
    for para in s.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", line, word)
            };
            if text_width_mm(&candidate, size, bold) > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    lines
}

#[derive(Clone, Copy)]
enum Align {
    Left,
    Right,
}

struct Col {
    title: &'static str,
    width: f32,
    align: Align,
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(Rgb::new(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        None,
    ))
}

const INK: (u8, u8, u8) = (30, 41, 59);
const MUTED: (u8, u8, u8) = (100, 116, 139);
const ACCENT: (u8, u8, u8) = (15, 118, 110);
const ACCENT_BG: (u8, u8, u8) = (220, 242, 239);
const AMBER: (u8, u8, u8) = (146, 64, 14);
const AMBER_BG: (u8, u8, u8) = (254, 243, 199);
const RULE: (u8, u8, u8) = (226, 232, 240);
const RED: (u8, u8, u8) = (185, 28, 28);

struct Writer {
    doc: PdfDocumentReference,
    font: IndirectFontRef,
    bold: IndirectFontRef,
    pages: Vec<(PdfPageIndex, PdfLayerIndex)>,
    layer: PdfLayerReference,
    y: f32,
}

impl Writer {
    fn new(title: &str) -> Result<Writer, String> {
        let (doc, p, l) = PdfDocument::new(clean(title), Mm(PAGE_W), Mm(PAGE_H), "Layer 1");
        let font = doc
            .add_builtin_font(BuiltinFont::Helvetica)
            .map_err(|e| e.to_string())?;
        let bold = doc
            .add_builtin_font(BuiltinFont::HelveticaBold)
            .map_err(|e| e.to_string())?;
        let layer = doc.get_page(p).get_layer(l);
        Ok(Writer {
            doc,
            font,
            bold,
            pages: vec![(p, l)],
            layer,
            y: PAGE_H - MARGIN,
        })
    }

    fn new_page(&mut self) {
        let (p, l) = self.doc.add_page(Mm(PAGE_W), Mm(PAGE_H), "Layer 1");
        self.layer = self.doc.get_page(p).get_layer(l);
        self.pages.push((p, l));
        self.y = PAGE_H - MARGIN;
    }

    /// Make sure `h` mm fit before the bottom margin (leaving room for the footer).
    fn need(&mut self, h: f32) {
        if self.y - h < MARGIN + 8.0 {
            self.new_page();
        }
    }

    fn color(&self, c: (u8, u8, u8)) {
        self.layer.set_fill_color(rgb(c.0, c.1, c.2));
    }

    fn text_at(&self, s: &str, size: f32, x: f32, y: f32, bold: bool, c: (u8, u8, u8)) {
        self.color(c);
        let f = if bold { &self.bold } else { &self.font };
        self.layer.use_text(clean(s), size, Mm(x), Mm(y), f);
    }

    fn rect(&self, x: f32, y: f32, w: f32, h: f32, c: (u8, u8, u8)) {
        self.color(c);
        self.layer.add_rect(
            Rect::new(Mm(x), Mm(y), Mm(x + w), Mm(y + h)).with_mode(path::PaintMode::Fill),
        );
    }

    fn rule(&mut self) {
        self.rect(MARGIN, self.y, CONTENT_W, 0.3, RULE);
        self.y -= 2.0;
    }

    fn para(&mut self, s: &str, size: f32, bold: bool, c: (u8, u8, u8), indent: f32) {
        let lh = size * 0.42 + 1.2;
        for line in wrap(s, size, bold, CONTENT_W - indent) {
            self.need(lh);
            self.y -= lh;
            self.text_at(&line, size, MARGIN + indent, self.y, bold, c);
        }
    }

    fn heading(&mut self, s: &str) {
        self.need(16.0);
        self.y -= 6.0;
        self.rect(MARGIN, self.y - 1.6, 1.4, 6.0, ACCENT);
        self.text_at(s, 13.0, MARGIN + 3.5, self.y, true, ACCENT);
        self.y -= 3.0;
    }

    fn table(&mut self, cols: &[Col], rows: &[Vec<String>], highlight: &[bool]) {
        let size = 9.0;
        let lh = 5.2;
        let draw_header = |w: &mut Writer| {
            w.need(lh * 2.0);
            w.y -= lh;
            w.rect(MARGIN, w.y - 1.6, CONTENT_W, lh, ACCENT_BG);
            let mut x = MARGIN + 1.5;
            for c in cols {
                let tx = match c.align {
                    Align::Left => x,
                    Align::Right => x + c.width - 3.0 - text_width_mm(c.title, 8.5, true),
                };
                w.text_at(c.title, 8.5, tx, w.y, true, ACCENT);
                x += c.width;
            }
        };
        draw_header(self);
        for (ri, row) in rows.iter().enumerate() {
            // Wrap left-aligned cells.
            let cells: Vec<Vec<String>> = row
                .iter()
                .zip(cols)
                .map(|(s, c)| match c.align {
                    Align::Left => wrap(s, size, false, c.width - 3.0),
                    Align::Right => vec![s.clone()],
                })
                .collect();
            let n = cells.iter().map(|c| c.len()).max().unwrap_or(1);
            if self.y - lh * (n as f32) < MARGIN + 8.0 {
                self.new_page();
                draw_header(self);
            }
            let hl = highlight.get(ri).copied().unwrap_or(false);
            if hl {
                self.rect(
                    MARGIN,
                    self.y - lh * n as f32 - 1.6,
                    CONTENT_W,
                    lh * n as f32,
                    AMBER_BG,
                );
            }
            let top = self.y;
            let mut x = MARGIN + 1.5;
            for (ci, c) in cols.iter().enumerate() {
                for (li, line) in cells[ci].iter().enumerate() {
                    let y = top - lh * (li as f32 + 1.0);
                    let tx = match c.align {
                        Align::Left => x,
                        Align::Right => x + c.width - 3.0 - text_width_mm(line, size, false),
                    };
                    self.text_at(line, size, tx, y, false, INK);
                }
                x += c.width;
            }
            self.y = top - lh * n as f32;
            self.rect(MARGIN, self.y - 1.7, CONTENT_W, 0.2, RULE);
        }
        self.y -= 2.0;
    }

    fn footer(&self, group_name: &str) {
        let n = self.pages.len();
        for (i, (p, l)) in self.pages.iter().enumerate() {
            let layer = self.doc.get_page(*p).get_layer(*l);
            layer.set_fill_color(rgb(MUTED.0, MUTED.1, MUTED.2));
            let left = clean(&format!("GroupKeep • {}", group_name));
            layer.use_text(left, 8.0, Mm(MARGIN), Mm(MARGIN - 4.0), &self.font);
            let right = format!("Page {} of {}", i + 1, n);
            let w = text_width_mm(&right, 8.0, false);
            layer.use_text(
                right,
                8.0,
                Mm(PAGE_W - MARGIN - w),
                Mm(MARGIN - 4.0),
                &self.font,
            );
        }
    }
}

fn shares_cell(g: &Group, p: &Person) -> String {
    if g.group_type == GroupType::CommunityEvent {
        format!("{} people", trim_num(p.shares))
    } else {
        trim_num(p.shares)
    }
}

/// Build the PDF report and return its bytes.
pub fn render(g: &Group, r: &Report) -> Result<Vec<u8>, String> {
    let cur = g.currency;
    let money = |v: i64| cur.format(v);
    let title = if g.name.trim().is_empty() {
        "Group".to_string()
    } else {
        g.name.clone()
    };
    let mut w = Writer::new(&title)?;

    // Header band.
    w.rect(0.0, PAGE_H - 34.0, PAGE_W, 34.0, ACCENT);
    w.text_at(
        "GROUPKEEP • MONEYKEEP FAMILY",
        8.0,
        MARGIN,
        PAGE_H - 11.0,
        true,
        (204, 251, 241),
    );
    w.text_at(&title, 20.0, MARGIN, PAGE_H - 20.0, true, (255, 255, 255));
    let sub = format!(
        "{} • {} • Prepared {}",
        g.group_type.label(),
        cur.label(),
        chrono::Local::now().format("%b %-d, %Y")
    );
    w.text_at(&sub, 9.5, MARGIN, PAGE_H - 27.5, false, (220, 252, 245));
    w.y = PAGE_H - 40.0;
    if !g.description.trim().is_empty() {
        w.para(&g.description, 10.0, false, INK, 0.0);
    }
    if g.group_type.uses_days() {
        let labels = g.day_labels();
        w.para(
            &format!(
                "Trip: {} to {} ({} days)",
                labels.first().cloned().unwrap_or_default(),
                labels.last().cloned().unwrap_or_default(),
                g.num_days
            ),
            10.0,
            false,
            MUTED,
            0.0,
        );
    }

    // Key figures.
    w.need(18.0);
    w.y -= 4.0;
    let boxes = [
        ("Total spent", money(r.total_spent)),
        ("Donations / sponsors", money(r.total_contributions)),
        (
            if g.group_type == GroupType::CommunityEvent {
                "Households"
            } else {
                "People"
            },
            g.people.len().to_string(),
        ),
        ("Overrides", (g.overrides.len()).to_string()),
    ];
    let bw = (CONTENT_W - 9.0) / 4.0;
    for (i, (label, value)) in boxes.iter().enumerate() {
        let x = MARGIN + i as f32 * (bw + 3.0);
        let hl = *label == "Overrides" && !g.overrides.is_empty();
        w.rect(
            x,
            w.y - 13.0,
            bw,
            13.0,
            if hl { AMBER_BG } else { ACCENT_BG },
        );
        w.text_at(
            label,
            7.5,
            x + 2.5,
            w.y - 4.5,
            true,
            if hl { AMBER } else { MUTED },
        );
        w.text_at(value, 12.0, x + 2.5, w.y - 10.5, true, INK);
    }
    w.y -= 16.0;

    // Members.
    w.heading(if g.group_type == GroupType::CommunityEvent {
        "Households"
    } else {
        "People"
    });
    let mut cols = vec![
        Col {
            title: "Name",
            width: 45.0,
            align: Align::Left,
        },
        Col {
            title: "Description",
            width: 85.0,
            align: Align::Left,
        },
        Col {
            title: g.group_type.shares_label(),
            width: 26.0,
            align: Align::Right,
        },
    ];
    if g.group_type.uses_days() {
        cols[1].width = 65.0;
        cols.push(Col {
            title: "Days there",
            width: 20.0,
            align: Align::Right,
        });
    }
    let used: f32 = cols.iter().map(|c| c.width).sum();
    cols[1].width += CONTENT_W - used;
    let rows: Vec<Vec<String>> = g
        .people
        .iter()
        .map(|p| {
            let mut row = vec![
                g.person_name(p.id),
                p.description.clone(),
                shares_cell(g, p),
            ];
            if g.group_type.uses_days() {
                let n = p
                    .attendance
                    .iter()
                    .take(g.num_days as usize)
                    .filter(|a| **a)
                    .count();
                row.push(format!("{} of {}", n, g.num_days));
            }
            row
        })
        .collect();
    w.table(&cols, &rows, &[]);

    // Overrides — clearly visible, near the top.
    if !g.overrides.is_empty() {
        w.heading("Overrides");
        w.para(
            "Manual changes to the calculated split. Each one's difference was re-split among the others in proportion to their shares.",
            8.5,
            false,
            MUTED,
            0.0,
        );
        for o in &g.overrides {
            let scope = match o.scope {
                OverrideScope::Group => "Whole group".to_string(),
                OverrideScope::Expense(id) => g
                    .expense(id)
                    .map(|e| e.description.clone())
                    .unwrap_or_else(|| "(removed expense)".into()),
            };
            let effect = r
                .all_overrides()
                .into_iter()
                .find(|(_, a)| a.override_id == o.id)
                .map(|(_, a)| a.effect.clone())
                .unwrap_or_else(|| {
                    format!("{} (not applied — see notes)", describe_override(o, g))
                });
            let desc = if o.description.trim().is_empty() {
                "(no description)".to_string()
            } else {
                o.description.clone()
            };
            let line2 = format!("{} • {} • {}", g.person_name(o.person), scope, effect);
            let l1 = wrap(&desc, 10.0, true, CONTENT_W - 8.0);
            let l2 = wrap(&line2, 9.0, false, CONTENT_W - 8.0);
            let h = 3.0 + l1.len() as f32 * 5.0 + l2.len() as f32 * 4.5;
            w.need(h + 2.0);
            w.y -= 1.0;
            w.rect(MARGIN, w.y - h, CONTENT_W, h, AMBER_BG);
            w.rect(MARGIN, w.y - h, 1.2, h, AMBER);
            let mut y = w.y - 1.0;
            for l in l1 {
                y -= 5.0;
                w.text_at(&l, 10.0, MARGIN + 4.0, y, true, AMBER);
            }
            for l in l2 {
                y -= 4.5;
                w.text_at(&l, 9.0, MARGIN + 4.0, y, false, INK);
            }
            w.y -= h + 1.5;
        }
    }

    // Expenses.
    w.heading("Expenses");
    for (e, er) in g.expenses.iter().zip(&r.expenses) {
        w.need(30.0);
        w.y -= 6.0;
        w.text_at(&er.description, 11.5, MARGIN, w.y, true, INK);
        let t = money(er.total);
        w.text_at(
            &t,
            11.5,
            PAGE_W - MARGIN - text_width_mm(&t, 11.5, true),
            w.y,
            true,
            INK,
        );
        let payer = er
            .payer
            .map(|p| g.person_name(p))
            .unwrap_or_else(|| "nobody (not counted)".into());
        let mut meta = format!("Paid by {} • Split: {}", payer, e.method.label());
        if !e.date.trim().is_empty() {
            meta = format!("{} • {}", e.date, meta);
        }
        w.para(&meta, 9.0, false, MUTED, 0.0);
        if e.method == Method::ByItem {
            let items: Vec<String> = e
                .items
                .iter()
                .map(|it| {
                    let who = if it.shared_by.is_empty() {
                        "everyone".to_string()
                    } else {
                        it.shared_by
                            .iter()
                            .map(|p| g.person_name(*p))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    format!(
                        "{} {} ({})",
                        if it.description.is_empty() {
                            "Item"
                        } else {
                            &it.description
                        },
                        money(it.amount),
                        who
                    )
                })
                .collect();
            let tip = match e.tip {
                Tip::None => String::new(),
                Tip::Percent(p) => format!(" • Tip {}% = {}", trim_num(p), money(e.tip_amount())),
                Tip::Amount(a) => format!(" • Tip {}", money(a)),
            };
            w.para(
                &format!(
                    "Items: {}. Subtotal {} • Tax {}{}",
                    items.join("; "),
                    money(e.item_subtotal()),
                    money(e.tax),
                    tip
                ),
                8.5,
                false,
                MUTED,
                0.0,
            );
        }
        if e.method == Method::ByAttendance {
            let labels = g.day_labels();
            let days: Vec<String> = labels
                .iter()
                .enumerate()
                .filter(|(i, _)| e.days.is_empty() || e.days.get(*i).copied().unwrap_or(false))
                .map(|(_, l)| l.clone())
                .collect();
            w.para(
                &format!("Covers: {}", days.join(", ")),
                8.5,
                false,
                MUTED,
                0.0,
            );
        }
        let cols = [
            Col {
                title: "Person",
                width: 60.0,
                align: Align::Left,
            },
            Col {
                title: "By method",
                width: 30.0,
                align: Align::Right,
            },
            Col {
                title: "Pays",
                width: 30.0,
                align: Align::Right,
            },
            Col {
                title: "Note",
                width: CONTENT_W - 120.0,
                align: Align::Left,
            },
        ];
        let mut rows = vec![];
        let mut hl = vec![];
        for l in &er.lines {
            let mut notes = vec![];
            if l.excluded {
                notes.push("Excluded (override)".to_string());
            } else if l.overridden {
                notes.push("Override".to_string());
            }
            for rn in er.rounding.iter().filter(|rn| rn.person == l.person) {
                notes.push(format!(
                    "{} rounding",
                    if rn.amount > 0 {
                        format!("+{}", cur.minor_name(rn.amount))
                    } else {
                        cur.minor_name(rn.amount)
                    }
                ));
            }
            if Some(l.person) == er.payer {
                notes.push("Payer".into());
            }
            rows.push(vec![
                g.person_name(l.person),
                if l.excluded {
                    "-".into()
                } else {
                    money(l.base)
                },
                money(l.amount),
                notes.join(" • "),
            ]);
            hl.push(l.overridden);
        }
        w.table(&cols, &rows, &hl);
        for o in &er.overrides {
            w.para(
                &format!(
                    "Override - {}: {} ({})",
                    g.person_name(o.person),
                    o.description,
                    o.effect
                ),
                8.5,
                false,
                AMBER,
                2.0,
            );
        }
        for wn in &er.warnings {
            w.para(&format!("Note: {}", wn), 8.5, false, RED, 2.0);
        }
    }

    // Donations.
    if !g.contributions.is_empty() {
        w.heading("Donations and sponsors");
        let cols = [
            Col {
                title: "Description",
                width: 70.0,
                align: Align::Left,
            },
            Col {
                title: "From",
                width: 45.0,
                align: Align::Left,
            },
            Col {
                title: "Held by",
                width: 35.0,
                align: Align::Left,
            },
            Col {
                title: "Amount",
                width: CONTENT_W - 150.0,
                align: Align::Right,
            },
        ];
        let rows: Vec<Vec<String>> = g
            .contributions
            .iter()
            .map(|c| {
                vec![
                    c.description.clone(),
                    c.from.clone(),
                    c.received_by
                        .map(|p| g.person_name(p))
                        .unwrap_or_else(|| "nobody".into()),
                    money(c.amount),
                ]
            })
            .collect();
        w.table(&cols, &rows, &[]);
        w.para(
            "Donations lower each member's total in proportion to what they owe.",
            8.5,
            false,
            MUTED,
            0.0,
        );
    }

    // Summary.
    w.heading("Summary");
    let has_credit = r.total_contributions != 0;
    let mut cols = vec![
        Col {
            title: "Name",
            width: 50.0,
            align: Align::Left,
        },
        Col {
            title: "Paid",
            width: 28.0,
            align: Align::Right,
        },
        Col {
            title: "Share",
            width: 28.0,
            align: Align::Right,
        },
    ];
    if has_credit {
        cols.push(Col {
            title: "Credit",
            width: 24.0,
            align: Align::Right,
        });
        cols.push(Col {
            title: "Holding",
            width: 24.0,
            align: Align::Right,
        });
    }
    let used: f32 = cols.iter().map(|c| c.width).sum();
    cols.push(Col {
        title: "Balance",
        width: CONTENT_W - used,
        align: Align::Right,
    });
    let rows: Vec<Vec<String>> = r
        .people
        .iter()
        .map(|p| {
            let mut row = vec![p.name.clone(), money(p.paid), money(p.owed)];
            if has_credit {
                row.push(money(p.credit));
                row.push(money(p.holding));
            }
            let bal = if p.balance > 0 {
                format!("gets back {}", money(p.balance))
            } else if p.balance < 0 {
                format!("owes {}", money(-p.balance))
            } else {
                "settled".into()
            };
            row.push(bal);
            row
        })
        .collect();
    w.table(&cols, &rows, &[]);

    w.heading("Settle up");
    if r.transfers.is_empty() {
        w.para(
            "Everyone is even. No payments needed.",
            10.0,
            false,
            INK,
            0.0,
        );
    } else {
        for t in &r.transfers {
            w.need(8.0);
            w.y -= 6.5;
            w.rect(MARGIN, w.y - 2.0, CONTENT_W, 6.5, ACCENT_BG);
            let s = format!("{}  pays  {}", g.person_name(t.from), g.person_name(t.to));
            w.text_at(&s, 10.5, MARGIN + 3.0, w.y, false, INK);
            let m = money(t.amount);
            w.text_at(
                &m,
                10.5,
                PAGE_W - MARGIN - 3.0 - text_width_mm(&m, 10.5, true),
                w.y,
                true,
                INK,
            );
            w.y -= 1.0;
        }
    }

    let rounding = r.all_rounding();
    if !rounding.is_empty() {
        w.heading("Rounding");
        w.para("Splits that do not divide evenly leave a few pennies over. Those always go to the payer:", 8.5, false, MUTED, 0.0);
        for rn in rounding {
            let amt = if rn.amount > 0 {
                format!("+{}", cur.minor_name(rn.amount))
            } else {
                cur.minor_name(rn.amount)
            };
            w.para(
                &format!("• {} {} — {}", g.person_name(rn.person), amt, rn.context),
                9.0,
                false,
                INK,
                2.0,
            );
        }
    }

    if !r.warnings.is_empty() {
        w.heading("Notes");
        for wn in &r.warnings {
            w.para(&format!("• {}", wn), 9.0, false, RED, 2.0);
        }
    }

    w.y -= 4.0;
    w.rule();

    w.footer(&title);
    w.doc.save_to_bytes().map_err(|e| e.to_string())
}
