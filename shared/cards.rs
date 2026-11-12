use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub cards: Vec<ConnectedCard>,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectedCard {
    pub id: String,
    pub item_id: String,
    pub institution: String,
    pub name: String,
    pub color: [u8; 3],
    pub last_four: Option<String>,
    pub brand: Option<String>,
    pub currency: String,
    pub balance: Option<i64>,
    pub limit: Option<i64>,
    pub available: Option<i64>,
    pub due: Option<String>,
    pub updated_at: Option<String>,
    pub bills: Vec<ConnectedBill>,
    pub bills_available: bool,
    /// Posição (0/1) do membro dono da conexão; preenchida pelo backend a partir do banco.
    #[serde(default)]
    pub owner: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectedBill {
    pub due: String,
    pub total: Option<i64>,
    pub currency: String,
    /// Marcada como paga no app (a Pluggy não informa); preservada ao recarregar.
    #[serde(default)]
    pub paid: bool,
}

pub fn cents(value: &Value) -> Option<i64> {
    let amount = value.as_f64()? * 100.0;
    // Abaixo do limite de precisão inteira do JSON; não converter valores enormes em saldo válido.
    (amount.is_finite() && amount.abs() < 9_007_199_254_740_991.0).then(|| amount.round() as i64)
}

pub fn iso_date(value: &str) -> Option<&str> {
    let date = value.get(..10)?;
    let bytes = date.as_bytes();
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return None;
    }
    let y: u32 = date[..4].parse().ok()?;
    let m: u32 = date[5..7].parse().ok()?;
    let d: u32 = date[8..].parse().ok()?;
    let days = match m {
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1..=12 => 31,
        _ => return None,
    };
    (y >= 1900 && y <= 2100 && (1..=days).contains(&d)).then_some(date)
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.chars().filter(|c| !c.is_control()).take(120).collect())
}

pub fn identify(connector: &Value, account: &Value) -> (String, [u8; 3]) {
    let connector_name = connector["name"].as_str().unwrap_or("");
    let proxy = connector["id"] == 200
        || connector_name
            .replace(' ', "")
            .eq_ignore_ascii_case("MeuPluggy");
    let direct = !connector_name.is_empty() && !proxy;
    let source = if direct {
        connector_name.to_lowercase()
    } else {
        format!(
            "{} {}",
            account["marketingName"].as_str().unwrap_or(""),
            account["name"].as_str().unwrap_or("")
        )
        .to_lowercase()
    };
    for (aliases, name, color) in [
        (&["nubank", "nu pagamentos"][..], "Nubank", [130, 10, 209]),
        (
            &["banco inter", "inter mastercard"][..],
            "Inter",
            [220, 83, 0],
        ),
        (&["itaú", "itau"][..], "Itaú", [203, 73, 0]),
        (&["santander"][..], "Santander", [190, 0, 0]),
        (&["bradesco"][..], "Bradesco", [176, 12, 54]),
        (&["c6 bank", "c6bank", "c6 "][..], "C6 Bank", [40, 40, 43]),
        (
            &["banco do brasil", "ourocard"][..],
            "Banco do Brasil",
            [15, 49, 137],
        ),
        (&["caixa"][..], "Caixa", [0, 86, 143]),
        (&["picpay"][..], "PicPay", [0, 104, 62]),
        (
            &["mercado pago", "mercadopago"][..],
            "Mercado Pago",
            [0, 99, 146],
        ),
    ] {
        if aliases.iter().any(|alias| source.contains(alias)) {
            return (name.into(), color);
        }
    }
    if connector_name.eq_ignore_ascii_case("inter") {
        return ("Inter".into(), [220, 83, 0]);
    }
    let name = if direct {
        text(&connector["name"])
    } else {
        text(&account["marketingName"]).or_else(|| text(&account["name"]))
    }
    .unwrap_or_else(|| "Cartão conectado".into());
    let color = if direct {
        connector["primaryColor"].as_str().and_then(|s| {
            let s = s.trim_start_matches('#');
            if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            Some([
                u8::from_str_radix(&s[..2], 16).ok()?,
                u8::from_str_radix(&s[2..4], 16).ok()?,
                u8::from_str_radix(&s[4..], 16).ok()?,
            ])
        })
    } else {
        None
    }
    .unwrap_or([48, 64, 88]);
    (name, color)
}

impl ConnectedCard {
    pub fn from_pluggy(
        item_id: &str,
        connector: &Value,
        account: &Value,
        bills: Option<&Value>,
    ) -> Option<Self> {
        if account["type"] != "CREDIT" {
            return None;
        }
        let (institution, color) = identify(connector, account);
        let digits: String = account["number"]
            .as_str()
            .unwrap_or("")
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        let credit = &account["creditData"];
        let currency =
            text(&account["currencyCode"]).unwrap_or_else(|| "Moeda não informada".into());
        Some(Self {
            id: account["id"].as_str()?.into(),
            item_id: item_id.into(),
            institution,
            name: text(&account["marketingName"])
                .or_else(|| text(&account["name"]))
                .unwrap_or_else(|| "Cartão de crédito".into()),
            color,
            last_four: (digits.len() >= 4).then(|| digits[digits.len() - 4..].into()),
            brand: text(&credit["brand"]),
            currency,
            balance: cents(&account["balance"]),
            limit: cents(&credit["creditLimit"]),
            available: cents(&credit["availableCreditLimit"]),
            due: credit["balanceDueDate"]
                .as_str()
                .and_then(iso_date)
                .map(str::to_owned),
            updated_at: text(&account["updatedAt"]),
            bills_available: bills.is_some(),
            bills: bills
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|bill| {
                    Some(ConnectedBill {
                        due: iso_date(bill["dueDate"].as_str()?)?.into(),
                        total: cents(&bill["totalAmount"]),
                        currency: text(&bill["totalAmountCurrencyCode"])
                            .unwrap_or_else(|| "Moeda não informada".into()),
                        paid: false,
                    })
                })
                .collect(),
            owner: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_real_fields_without_inventing_limits_or_a_bill() {
        let card = ConnectedCard::from_pluggy("one", &json!({"name":"MeuPluggy","primaryColor":"ef294b"}), &json!({
            "id":"card", "type":"CREDIT", "marketingName":"Nubank Mastercard", "number":"1234567891234", "balance":123.45,
            "creditData":{"availableCreditLimit":0,"balanceDueDate":"2026-11-25T00:00:00Z"}
        }), None).unwrap();
        assert_eq!(card.institution, "Nubank");
        assert_eq!(card.color, [130, 10, 209]);
        assert_eq!(card.last_four.as_deref(), Some("1234"));
        assert_eq!(card.balance, Some(12345));
        assert_eq!(card.available, Some(0));
        assert_eq!(card.limit, None);
        assert!(card.bills.is_empty());
        assert_eq!(card.due.as_deref(), Some("2026-11-25"));
        assert!(!card.bills_available);
        let json = serde_json::to_string(&card).unwrap();
        assert!(!json.contains("123456789"));
    }

    #[test]
    fn explicit_connection_identity_resolves_generic_platinum_without_guessing() {
        let account = json!({"name":"platinum", "marketingName":null});
        assert_eq!(
            identify(&json!({"id":200,"name":"Meu Pluggy"}), &account).1,
            [48, 64, 88]
        );
        assert_eq!(
            identify(&json!({"name":"Nubank"}), &account),
            ("Nubank".into(), [130, 10, 209])
        );
    }

    #[test]
    fn money_dates_and_identity_handle_missing_and_foreign_data() {
        assert_eq!(cents(&json!(19.99)), Some(1999));
        assert_eq!(cents(&json!(-0.29)), Some(-29));
        assert_eq!(cents(&Value::Null), None);
        assert_eq!(cents(&json!(1e100)), None);
        assert!(iso_date("2026-02-29").is_none());
        assert!(iso_date("2028-02-29").is_some());
        assert_eq!(
            identify(
                &json!({"name":"MeuPluggy","primaryColor":"ef294b"}),
                &json!({"name":"Mastercard"})
            )
            .1,
            [48, 64, 88]
        );
        assert_eq!(
            identify(
                &json!({"name":"Outro Banco","primaryColor":"#123456"}),
                &Value::Null
            )
            .1,
            [18, 52, 86]
        );
        let card = ConnectedCard::from_pluggy("one", &Value::Null, &json!({"id":"c","type":"CREDIT","currencyCode":"USD"}), Some(&json!([
            {"dueDate":"2026-11-20","totalAmount":20.51,"totalAmountCurrencyCode":"USD"}, {"dueDate":"bad","totalAmount":10}
        ]))).unwrap();
        assert_eq!(card.currency, "USD");
        assert_eq!(card.bills.len(), 1);
        assert_eq!(card.bills[0].total, Some(2051));
    }
}
