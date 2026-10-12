//! Divisao de texto em frases. Porta fiel do `split_text` de `tts/server.py`
//! (contando caracteres, nao bytes, como o Python).

/// Tamanho maximo de uma fatia, igual ao `MAX_CHUNK` do servidor.
pub const MAX_CHUNK: usize = 250;

/// Com `primeira_curta`, a primeira fatia acima disso e cortada na primeira virgula.
pub const PRIMEIRA_CURTA: usize = 60;

const FIM_DE_FRASE: [char; 6] = ['.', '!', '?', '…', ';', ':'];

/// `str.isspace()` / `\s` do Python (Unicode): `is_whitespace` mais 0x1c..=0x1f.
fn espaco(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn strip_espacos(s: &[char]) -> &[char] {
    let i = s.iter().position(|&c| !espaco(c)).unwrap_or(s.len());
    let f = s.iter().rposition(|&c| !espaco(c)).map_or(i, |p| p + 1);
    &s[i..f]
}

fn strip_espaco_virgula(s: &[char]) -> &[char] {
    let n = |c: &char| *c == ' ' || *c == ',';
    let i = s.iter().position(|c| !n(c)).unwrap_or(s.len());
    let f = s.iter().rposition(|c| !n(c)).map_or(i, |p| p + 1);
    &s[i..f]
}

fn rfind_virgula_espaco(s: &[char], limite: usize) -> Option<usize> {
    let fim = limite.min(s.len());
    (0..fim.saturating_sub(1))
        .rev()
        .find(|&i| s[i] == ',' && s[i + 1] == ' ')
}

fn rfind_espaco(s: &[char], limite: usize) -> Option<usize> {
    (0..limite.min(s.len())).rev().find(|&i| s[i] == ' ')
}

/// Colapsa `[ \t\r\f\v]+` em um espaco (o `\n` fica).
fn normalizar(texto: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(texto.len());
    let mut em_run = false;
    for c in texto.chars() {
        if matches!(c, ' ' | '\t' | '\r' | '\u{0c}' | '\u{0b}') {
            if !em_run {
                out.push(' ');
            }
            em_run = true;
        } else {
            out.push(c);
            em_run = false;
        }
    }
    out
}

/// `re.split(r"(?<=[.!?…;:])\s+", linha)`.
fn separar_sentencas(linha: &[char]) -> Vec<&[char]> {
    let mut partes = Vec::new();
    let mut ini = 0;
    let mut i = 0;
    while i < linha.len() {
        if espaco(linha[i]) && i > 0 && FIM_DE_FRASE.contains(&linha[i - 1]) {
            let mut j = i;
            while j < linha.len() && espaco(linha[j]) {
                j += 1;
            }
            partes.push(&linha[ini..i]);
            ini = j;
            i = j;
        } else {
            i += 1;
        }
    }
    partes.push(&linha[ini..]);
    partes
}

/// Quebra `texto` em frases curtas, como o `split_text` do servidor.
///
/// Com `primeira_curta`, se a primeira fatia passar de [`PRIMEIRA_CURTA`]
/// caracteres ela e cortada na primeira virgula (o resto vira a fatia seguinte),
/// para o primeiro audio ficar pronto mais cedo.
pub fn dividir_frases(texto: &str, primeira_curta: bool) -> Vec<String> {
    let norm = normalizar(texto);
    let texto = strip_espacos(&norm);
    let mut pecas: Vec<String> = Vec::new();
    for linha in texto.split(|&c| c == '\n') {
        // `\n+`: linhas vazias consecutivas somem; a unica sobra possivel e vazia e e descartada abaixo.
        let linha = strip_espacos(linha);
        for sent in separar_sentencas(linha) {
            let mut sent = strip_espacos(sent);
            while sent.len() > MAX_CHUNK {
                let corte = [
                    rfind_virgula_espaco(sent, MAX_CHUNK),
                    rfind_espaco(sent, MAX_CHUNK),
                ]
                .into_iter()
                .flatten()
                .max()
                .filter(|&c| c > 0)
                .unwrap_or(MAX_CHUNK);
                pecas.push(strip_espaco_virgula(&sent[..corte]).iter().collect());
                sent = strip_espaco_virgula(&sent[corte..]);
            }
            if !sent.is_empty() {
                pecas.push(sent.iter().collect());
            }
        }
    }
    if primeira_curta {
        encurtar_primeira(&mut pecas);
    }
    pecas
}

fn encurtar_primeira(pecas: &mut Vec<String>) {
    let Some(primeira) = pecas.first() else {
        return;
    };
    if primeira.chars().count() <= PRIMEIRA_CURTA {
        return;
    }
    let chars: Vec<char> = primeira.chars().collect();
    let Some(v) = chars.iter().position(|&c| c == ',') else {
        return;
    };
    let cabeca: String = strip_espaco_virgula(&chars[..v]).iter().collect();
    let resto: String = strip_espaco_virgula(&chars[v + 1..]).iter().collect();
    if cabeca.is_empty() || resto.is_empty() {
        return;
    }
    pecas[0] = cabeca;
    pecas.insert(1, resto);
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Caso {
        texto: String,
        pedacos: Vec<String>,
    }

    #[test]
    fn identico_ao_split_text_do_python() {
        let casos: Vec<Caso> =
            serde_json::from_str(include_str!("../tests/fixtures/split_text.json")).unwrap();
        assert!(casos.len() >= 30);
        for (n, c) in casos.iter().enumerate() {
            assert_eq!(
                dividir_frases(&c.texto, false),
                c.pedacos,
                "caso {n}: {:?}",
                c.texto
            );
        }
    }

    #[test]
    fn nenhuma_fatia_passa_do_limite() {
        let casos: Vec<Caso> =
            serde_json::from_str(include_str!("../tests/fixtures/split_text.json")).unwrap();
        for c in &casos {
            for f in dividir_frases(&c.texto, true) {
                assert!(f.chars().count() <= MAX_CHUNK);
            }
        }
    }

    #[test]
    fn primeira_curta_corta_na_primeira_virgula() {
        let t = "Entendi o que voce pediu e vou fazer isso agora mesmo, depois eu confiro tudo, e aviso quando acabar.";
        assert_eq!(dividir_frases(t, false).len(), 1);
        let f = dividir_frases(t, true);
        assert_eq!(
            f,
            vec![
                "Entendi o que voce pediu e vou fazer isso agora mesmo",
                "depois eu confiro tudo, e aviso quando acabar."
            ]
        );
    }

    #[test]
    fn primeira_curta_nao_mexe_em_frase_curta_ou_sem_virgula() {
        assert_eq!(
            dividir_frases("Oi, tudo bem? Sim.", true),
            vec!["Oi, tudo bem?", "Sim."]
        );
        let sem = "palavra ".repeat(12);
        assert_eq!(dividir_frases(&sem, true), dividir_frases(&sem, false));
        assert!(dividir_frases("", true).is_empty());
    }
}
