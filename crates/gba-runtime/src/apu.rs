#[derive(Debug, Clone, Default)]
pub struct Apu {
    pub samples_generated: u64,
}

impl Apu {
    pub fn tick(&mut self, samples: u64) {
        self.samples_generated = self.samples_generated.wrapping_add(samples);
    }
}
