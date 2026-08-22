use gba_runtime::Ppu;

#[test]
fn affine_mode_public_api_smoke() {
    let mut ppu = Ppu::default();
    let vram = vec![0; 0x18000];
    let palette = vec![0; 0x400];
    let io = std::collections::HashMap::new();
    ppu.render_affine_mode_scanline(1 << 10 | 1, 0, &vram, &palette, &io);
}
