//! Gera o seed SQL a partir do mesmo mock usado pela interface.
use flounder::model::*;

fn id(number: u32) -> String {
    format!("00000000-0000-4000-8000-{number:012x}")
}
fn sql(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn date(value: Date) -> String {
    format!("{:04}-{:02}-{:02}", value.y, value.m, value.d)
}
fn main() {
    let data = Data::demo();
    let home = sql(&id(1));
    println!("BEGIN; SET LOCAL search_path TO finance, public;");
    println!(
        "INSERT INTO households(id,name) VALUES({home},'Demonstração') ON CONFLICT DO NOTHING;"
    );
    for (index, name) in data.names.iter().enumerate() {
        let member = sql(&id(100 + index as u32));
        println!(
            "INSERT INTO members(id,household_id,client_user_id,display_name,position) VALUES({member},{home},'client-{}',{},{index}) ON CONFLICT DO NOTHING;",
            index + 1,
            sql(name)
        );
        println!("INSERT INTO preferences(member_id) VALUES({member}) ON CONFLICT DO NOTHING;");
    }
    for (index, card) in data.cards.iter().enumerate() {
        println!(
            "INSERT INTO cards(id,household_id,owner_id,name,due_day,credit_limit_cents) VALUES({},{home},{},{},{},{}) ON CONFLICT DO NOTHING;",
            sql(&id(200 + index as u32)),
            sql(&id(100 + card.owner.idx() as u32)),
            sql(&card.name),
            card.due_day,
            card.limit
        );
    }
    for entry in &data.entries {
        let card = entry
            .card
            .map(|c| sql(&id(200 + c as u32)))
            .unwrap_or_else(|| "NULL".into());
        println!(
            "INSERT INTO entries(id,household_id,owner_id,description,amount_cents,entry_date,kind,category_key,shared,card_id,paid) VALUES({},{home},{},{},{},{},{},{},{},{card},{}) ON CONFLICT DO NOTHING;",
            sql(&id(1000 + entry.id)),
            sql(&id(100 + entry.owner.idx() as u32)),
            sql(&entry.desc),
            entry.amount,
            sql(&date(entry.date)),
            sql(if entry.kind == Kind::Income {
                "income"
            } else {
                "expense"
            }),
            sql(entry.category.key()),
            entry.shared,
            entry.paid
        );
    }
    for offset in -4..=1 {
        let month = Month::of(DEMO_TODAY).add(offset);
        for card in 0..data.cards.len() {
            let invoice = data.invoice(card, month);
            println!(
                "INSERT INTO card_invoices(id,household_id,card_id,due_date,total_cents,paid) VALUES({},{home},{},{},{},{}) ON CONFLICT DO NOTHING;",
                sql(&id(10_000 + (offset + 4) as u32 * 100 + card as u32)),
                sql(&id(200 + card as u32)),
                sql(&date(invoice.due)),
                invoice.total,
                invoice.paid
            );
        }
    }
    println!(
        "INSERT INTO reserve_goals(id,household_id,owner_id,name,opening_balance_cents,target_cents) VALUES({},{home},{},'Reserva',{},{}) ON CONFLICT DO NOTHING;",
        sql(&id(300)),
        sql(&id(100 + data.me.idx() as u32)),
        data.reserve_start,
        data.reserve_goal
    );
    println!("COMMIT;");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seed_quotes_text_and_preserves_mock_category_keys() {
        assert_eq!(sql("D'Ávila"), "'D''Ávila'");
        assert_eq!(date(DEMO_TODAY), "2026-11-10");
        for value in Category::ALL {
            assert!(!value.key().is_empty());
        }
    }
}
