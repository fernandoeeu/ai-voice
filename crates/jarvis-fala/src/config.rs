use std::path::PathBuf;
use std::time::Duration;

/// Configuracao da fila. Tudo tem padrao igual ao `fala-kokoro.sh`.
#[derive(Debug, Clone)]
pub struct Config {
    /// Voz do Kokoro quando o pedido nao traz uma (`JARVIS_TTS_VOICE`, padrao `pf_dora`).
    pub voz_padrao: String,
    pub vel_padrao: f32,
    /// Prazo da fila quando o pedido nao traz um (`lockf -t 300`).
    pub prazo_fila_padrao: Duration,
    /// Executavel do fallback (`/usr/bin/say`); configuravel para os testes.
    pub say_exe: PathBuf,
    /// Voz do fallback (`AI_VOICE_MAC_VOICE`, padrao `Luciana`).
    pub voz_mac: String,
    /// Caminho do `fala.log` (`JARVIS_FALA_LOG`).
    pub fala_log: PathBuf,
    /// Fade do `calar` na saida, em ms.
    pub fade_ms: u32,
    /// Primeira fatia curta (primeiro som mais cedo).
    pub primeira_curta: bool,
}

impl Default for Config {
    fn default() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        Self {
            voz_padrao: "pf_dora".into(),
            vel_padrao: 1.0,
            prazo_fila_padrao: Duration::from_secs(300),
            say_exe: "/usr/bin/say".into(),
            voz_mac: "Luciana".into(),
            fala_log: home.join(".local/share/jarvis/tts/logs/fala.log"),
            fade_ms: 30,
            primeira_curta: true,
        }
    }
}

impl Config {
    /// Padroes mais as variaveis de ambiente do script antigo.
    pub fn do_ambiente() -> Self {
        let mut c = Self::default();
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        if let Some(v) = env("JARVIS_TTS_VOICE") {
            c.voz_padrao = v;
        }
        if let Some(v) = env("AI_VOICE_MAC_VOICE") {
            c.voz_mac = v;
        }
        if let Some(v) = env("JARVIS_FALA_LOG") {
            c.fala_log = v.into();
        }
        c
    }
}
