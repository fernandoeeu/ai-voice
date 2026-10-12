//! Parser WAV minimo: PCM s16le, mono, 24 kHz. Qualquer outra coisa e erro.

use jarvis_protocolo::Pcm24k;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ErroWav {
    #[error("WAV curto demais ({0} bytes)")]
    Curto(usize),
    #[error("cabecalho RIFF/WAVE invalido")]
    Cabecalho,
    #[error("chunk fmt ausente")]
    SemFmt,
    #[error("chunk data ausente")]
    SemData,
    #[error("formato nao suportado: tag {tag}, {canais} canal(is), {taxa} Hz, {bits} bits")]
    Formato { tag: u16, canais: u16, taxa: u32, bits: u16 },
    #[error("WAV truncado: chunk {chunk} declara {declarado} bytes, so ha {presente}")]
    Truncado { chunk: &'static str, declarado: usize, presente: usize },
    #[error("tamanho de dados impar ({0})")]
    Impar(usize),
}

fn u16_le(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

fn u32_le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

pub fn decodificar(bytes: &[u8]) -> Result<Pcm24k, ErroWav> {
    if bytes.len() < 12 {
        return Err(ErroWav::Curto(bytes.len()));
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(ErroWav::Cabecalho);
    }
    let mut pos = 12;
    let mut fmt_ok = false;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let tam = u32_le(&bytes[pos + 4..pos + 8]) as usize;
        let corpo = pos + 8;
        let presente = bytes.len() - corpo;
        if id == b"fmt " {
            if tam < 16 || presente < 16 {
                return Err(ErroWav::Truncado { chunk: "fmt", declarado: tam.max(16), presente });
            }
            let f = &bytes[corpo..corpo + 16];
            let (tag, canais, taxa, bits) = (u16_le(&f[0..2]), u16_le(&f[2..4]), u32_le(&f[4..8]), u16_le(&f[14..16]));
            if tag != 1 || canais != 1 || taxa != Pcm24k::TAXA || bits != 16 {
                return Err(ErroWav::Formato { tag, canais, taxa, bits });
            }
            fmt_ok = true;
        } else if id == b"data" {
            if !fmt_ok {
                return Err(ErroWav::SemFmt);
            }
            if presente < tam {
                return Err(ErroWav::Truncado { chunk: "data", declarado: tam, presente });
            }
            if !tam.is_multiple_of(2) {
                return Err(ErroWav::Impar(tam));
            }
            let amostras = bytes[corpo..corpo + tam].as_chunks::<2>().0.iter().map(|p| i16::from_le_bytes(*p)).collect();
            return Ok(Pcm24k::new(amostras));
        }
        // Chunks sao alinhados em 2 bytes.
        pos = corpo.saturating_add(tam + (tam & 1));
    }
    Err(if fmt_ok { ErroWav::SemData } else { ErroWav::SemFmt })
}

/// Monta um WAV s16le mono (uso em testes e fakes).
pub fn codificar(amostras: &[i16], taxa: u32) -> Vec<u8> {
    let dados = (amostras.len() * 2) as u32;
    let mut v = Vec::with_capacity(44 + dados as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + dados).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&taxa.to_le_bytes());
    v.extend_from_slice(&(taxa * 2).to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&dados.to_le_bytes());
    for a in amostras {
        v.extend_from_slice(&a.to_le_bytes());
    }
    v
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ida_e_volta() {
        let a = vec![0, 1, -1, i16::MAX, i16::MIN];
        let p = decodificar(&codificar(&a, 24_000)).unwrap();
        assert_eq!(p.amostras, a);
        assert_eq!(p.taxa, 24_000);
    }

    #[test]
    fn rejeita_taxa_canais_bits() {
        assert!(matches!(decodificar(&codificar(&[1, 2], 16_000)), Err(ErroWav::Formato { taxa: 16_000, .. })));
        let mut w = codificar(&[1, 2], 24_000);
        w[22] = 2; // estereo
        assert!(matches!(decodificar(&w), Err(ErroWav::Formato { canais: 2, .. })));
        let mut w = codificar(&[1, 2], 24_000);
        w[34] = 8; // 8 bits
        assert!(matches!(decodificar(&w), Err(ErroWav::Formato { bits: 8, .. })));
    }

    #[test]
    fn rejeita_truncado_e_lixo() {
        let w = codificar(&[1, 2, 3, 4], 24_000);
        assert!(matches!(decodificar(&w[..w.len() - 3]), Err(ErroWav::Truncado { chunk: "data", .. })));
        assert!(matches!(decodificar(&w[..20]), Err(ErroWav::SemFmt | ErroWav::Truncado { .. })));
        assert!(matches!(decodificar(b"oi"), Err(ErroWav::Curto(2))));
        assert_eq!(decodificar(b"RIFFxxxxWAVELIST\0\0\0\0"), Err(ErroWav::SemFmt));
        assert_eq!(decodificar(b"XXXXxxxxWAVEfmt "), Err(ErroWav::Cabecalho));
    }

    #[test]
    fn pula_chunk_extra_e_aceita_vazio() {
        let w = codificar(&[7, 8], 24_000);
        let mut v = w[..36].to_vec();
        v.extend_from_slice(b"LIST\x03\x00\x00\x00abc\x00");
        v.extend_from_slice(&w[36..]);
        assert_eq!(decodificar(&v).unwrap().amostras, vec![7, 8]);
        assert!(decodificar(&codificar(&[], 24_000)).unwrap().amostras.is_empty());
    }
}
