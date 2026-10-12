//! Linha do `fala.log`:
//! `<iso-utc> pid=N motor=M chars=N audio_s=S rc=N dur_s=N ttfa_ms=N`
//! (as seis primeiras chaves iguais ao `fala-kokoro.sh`; `ttfa_ms` e novo).

use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct Registro<'a> {
    pub motor: &'a str,
    pub chars: u32,
    /// `None` quando o motor nao mediu (say): vira `?`.
    pub audio_s: Option<f64>,
    pub rc: i32,
    pub dur_s: u64,
    pub ttfa_ms: Option<u64>,
}

pub(crate) fn formatar(agora: SystemTime, pid: u32, r: &Registro<'_>) -> String {
    let audio = r
        .audio_s
        .map_or_else(|| "?".to_owned(), |a| format!("{a:.1}"));
    let ttfa = r.ttfa_ms.map_or_else(|| "?".to_owned(), |t| t.to_string());
    format!(
        "{} pid={pid} motor={} chars={} audio_s={audio} rc={} dur_s={} ttfa_ms={ttfa}",
        iso_utc(agora),
        r.motor,
        r.chars,
        r.rc,
        r.dur_s
    )
}

pub(crate) fn anexar(caminho: &Path, linha: &str) {
    if let Some(pai) = caminho.parent() {
        let _ = std::fs::create_dir_all(pai);
    }
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(caminho)
    {
        Ok(mut f) => {
            let _ = writeln!(f, "{linha}");
        }
        Err(e) => tracing::warn!("fala.log: {e}"),
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ` (UTC), como o `date -u +%Y-%m-%dT%H:%M:%SZ`.
pub(crate) fn iso_utc(t: SystemTime) -> String {
    let s = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (dias, resto) = ((s / 86_400) as i64, s % 86_400);
    // Algoritmo de Howard Hinnant (civil_from_days).
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let a = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{a:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        resto / 3600,
        resto % 3600 / 60,
        resto % 60
    )
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::time::Duration;

    #[test]
    fn iso_conhecido() {
        // 2026-10-12T03:04:05Z
        let t = UNIX_EPOCH + Duration::from_secs(1_791_774_245);
        assert_eq!(iso_utc(t), "2026-10-12T03:04:05Z");
        assert_eq!(iso_utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
    }
}
