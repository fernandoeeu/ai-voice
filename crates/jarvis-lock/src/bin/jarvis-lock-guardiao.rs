//! Binario de teste do guardiao: `jarvis-lock-guardiao <caminho>`.
use std::path::Path;

fn main() {
    let Some(caminho) = std::env::args_os().nth(1) else {
        std::process::exit(2);
    };
    std::process::exit(jarvis_lock::rodar_guardiao(Path::new(&caminho)));
}
