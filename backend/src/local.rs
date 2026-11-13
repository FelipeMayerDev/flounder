use std::{collections::HashMap, fs::OpenOptions, io::Write, path::Path};

pub(crate) fn values(path: &Path) -> Result<HashMap<String, String>, String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            let mut random = [0u8; 64];
            getrandom::getrandom(&mut random)
                .map_err(|_| "Não foi possível gerar os tokens locais.".to_owned())?;
            let token = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
            let contents = format!(
                "# Credenciais Pluggy: preencha localmente, sem enviar no chat.\nCLIENT_ID=\nCLIENT_SECRET=\nAPP_TOKEN_1={}\nAPP_TOKEN_2={}\nBIND_ADDR=127.0.0.1:3001\n# Código fixo de pareamento (opcional): quem tem ele conecta o app novo, sem flounder.env.\nPAIRING_CODE_1=\nPAIRING_CODE_2=\n# PostgreSQL do docker compose; a senha é POSTGRES_PASSWORD do .env da raiz.\n# DATABASE_URL=host=127.0.0.1 port=5433 user=flounder dbname=flounder password=…\nDATABASE_URL=\n",
                token(&random[..32]),
                token(&random[32..]),
            );
            file.write_all(contents.as_bytes())
                .map_err(|_| "Não foi possível gravar a configuração local.".to_owned())?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("Não foi possível abrir o arquivo de configuração local.".into()),
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|_| "Não foi possível ler a configuração local.".to_owned())?;
    parse(&contents)
}

fn parse(contents: &str) -> Result<HashMap<String, String>, String> {
    let mut values = HashMap::new();
    for (index, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, value) = line
            .split_once('=')
            .ok_or_else(|| format!("Linha {}: use NOME=valor.", index + 1))?;
        let name = name
            .trim()
            .strip_prefix("export ")
            .unwrap_or(name.trim())
            .trim();
        if ![
            "CLIENT_ID",
            "CLIENT_SECRET",
            "APP_TOKEN_1",
            "APP_TOKEN_2",
            "BIND_ADDR",
            "DATABASE_URL",
            "PAIRING_CODE_1",
            "PAIRING_CODE_2",
        ]
        .contains(&name)
        {
            continue;
        }
        let value = value.trim();
        let value = if value.starts_with(['\'', '"']) {
            let quote = value.chars().next().unwrap();
            if value.len() < 2 || !value.ends_with(quote) {
                return Err(format!("Linha {}: aspas não fechadas.", index + 1));
            }
            &value[1..value.len() - 1]
        } else {
            value
        };
        if values.insert(name.into(), value.into()).is_some() {
            return Err(format!("Linha {}: variável repetida.", index + 1));
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_values_are_literal_and_invalid_input_does_not_echo_secrets() {
        let values = parse(
            "# comment\nCLIENT_ID='test-id'\nCLIENT_SECRET=\"literal$(no-command)=value#suffix\"\n",
        )
        .unwrap();
        assert_eq!(values["CLIENT_SECRET"], "literal$(no-command)=value#suffix");
        assert!(
            parse(
                "API_KEY=unused
OTHER_VARIABLE=value"
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(
            parse("export CLIENT_ID=test-id").unwrap()["CLIENT_ID"],
            "test-id"
        );
        for input in [
            "CLIENT_SECRET",
            "CLIENT_SECRET='secret",
            "CLIENT_ID=a\nCLIENT_ID=b",
        ] {
            let error = parse(input).unwrap_err();
            assert!(!error.contains("secret"));
        }
    }

    #[test]
    fn bootstrap_creates_private_tokens_and_never_overwrites_configuration() {
        let name = format!(
            "flounder-env-{}-{}.env",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(name);
        let first = values(&path).unwrap();
        assert!(first["CLIENT_ID"].is_empty());
        assert_eq!(first["APP_TOKEN_1"].len(), 64);
        assert!(first["APP_TOKEN_1"] != first["APP_TOKEN_2"]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let contents = std::fs::read_to_string(&path)
            .unwrap()
            .replace("CLIENT_ID=\n", "CLIENT_ID=test-id\n");
        std::fs::write(&path, &contents).unwrap();
        assert_eq!(values(&path).unwrap()["CLIENT_ID"], "test-id");
        assert!(std::fs::read_to_string(&path).unwrap() == contents);
        std::fs::remove_file(path).unwrap();
    }
}
