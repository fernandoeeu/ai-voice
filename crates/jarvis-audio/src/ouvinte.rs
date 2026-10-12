//! Ouvintes CoreAudio: mudanca do dispositivo de saida padrao e da taxa
//! nominal do dispositivo. Apenas acendem uma flag; quem reconstroi o stream
//! e a thread gerente (fora do callback de audio).

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod imp {
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering::Release};

    use objc2_core_audio::{
        AudioObjectAddPropertyListener, AudioObjectGetPropertyData, AudioObjectID,
        AudioObjectPropertyAddress, AudioObjectRemovePropertyListener,
        kAudioDevicePropertyNominalSampleRate, kAudioHardwarePropertyDefaultOutputDevice,
        kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
    };

    const SISTEMA: AudioObjectID = kAudioObjectSystemObject as AudioObjectID;

    const ENDERECO_PADRAO: AudioObjectPropertyAddress = AudioObjectPropertyAddress {
        mSelector: kAudioHardwarePropertyDefaultOutputDevice,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    const ENDERECO_TAXA: AudioObjectPropertyAddress = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyNominalSampleRate,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };

    unsafe extern "C-unwind" fn ao_mudar(
        _obj: AudioObjectID,
        _n: u32,
        _enderecos: NonNull<AudioObjectPropertyAddress>,
        dados: *mut c_void,
    ) -> i32 {
        // SAFETY: `dados` e o ponteiro de `Arc::into_raw` guardado em `Ouvintes`,
        // vivo ate o `Drop` remover os ouvintes.
        let flag = unsafe { &*(dados as *const AtomicBool) };
        flag.store(true, Release);
        0
    }

    pub struct Ouvintes {
        dados: *const AtomicBool,
        dispositivo_taxa: Option<AudioObjectID>,
    }

    // SAFETY: so guarda um ponteiro para um AtomicBool (Sync) mantido vivo por Arc.
    unsafe impl Send for Ouvintes {}

    fn dispositivo_padrao() -> Option<AudioObjectID> {
        let mut id: AudioObjectID = 0;
        let mut tam = std::mem::size_of::<AudioObjectID>() as u32;
        let mut end = ENDERECO_PADRAO;
        // SAFETY: ponteiros validos para a duracao da chamada; tam bate com o tipo.
        let st = unsafe {
            AudioObjectGetPropertyData(
                SISTEMA,
                NonNull::from(&mut end),
                0,
                std::ptr::null(),
                NonNull::from(&mut tam),
                NonNull::from(&mut id).cast(),
            )
        };
        (st == 0 && id != 0).then_some(id)
    }

    impl Ouvintes {
        pub fn novo(flag: Arc<AtomicBool>) -> Self {
            let dados = Arc::into_raw(flag);
            let mut end = ENDERECO_PADRAO;
            // SAFETY: callback e dados validos ate o Drop.
            unsafe {
                AudioObjectAddPropertyListener(
                    SISTEMA,
                    NonNull::from(&mut end),
                    Some(ao_mudar),
                    dados as *mut c_void,
                );
            }
            let mut o = Self {
                dados,
                dispositivo_taxa: None,
            };
            o.rearmar();
            o
        }

        /// Registra o ouvinte de taxa no dispositivo padrao atual (remove do anterior).
        pub fn rearmar(&mut self) {
            self.remover_taxa();
            if let Some(id) = dispositivo_padrao() {
                let mut end = ENDERECO_TAXA;
                // SAFETY: idem `novo`.
                let st = unsafe {
                    AudioObjectAddPropertyListener(
                        id,
                        NonNull::from(&mut end),
                        Some(ao_mudar),
                        self.dados as *mut c_void,
                    )
                };
                if st == 0 {
                    self.dispositivo_taxa = Some(id);
                }
            }
        }

        fn remover_taxa(&mut self) {
            if let Some(id) = self.dispositivo_taxa.take() {
                let mut end = ENDERECO_TAXA;
                // SAFETY: mesmo callback e dados registrados em `rearmar`.
                unsafe {
                    AudioObjectRemovePropertyListener(
                        id,
                        NonNull::from(&mut end),
                        Some(ao_mudar),
                        self.dados as *mut c_void,
                    );
                }
            }
        }
    }

    impl Drop for Ouvintes {
        fn drop(&mut self) {
            self.remover_taxa();
            let mut end = ENDERECO_PADRAO;
            // SAFETY: remove o registro de `novo`; depois libera a referencia do Arc.
            unsafe {
                AudioObjectRemovePropertyListener(
                    SISTEMA,
                    NonNull::from(&mut end),
                    Some(ao_mudar),
                    self.dados as *mut c_void,
                );
                drop(Arc::from_raw(self.dados));
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub use imp::Ouvintes;

/// Fora do macOS nao ha ouvinte: o stream so reconstroi por erro do cpal.
#[cfg(not(target_os = "macos"))]
pub struct Ouvintes;
#[cfg(not(target_os = "macos"))]
impl Ouvintes {
    pub fn novo(_flag: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self
    }
    pub fn rearmar(&mut self) {}
}
