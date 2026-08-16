use eframe::egui::{self, Color32, TextureOptions};
use gba_core::{Cartridge, Gba, HEIGHT, WIDTH};
use std::{path::{Path, PathBuf}, time::{Duration, Instant}};

struct App { gba: Gba, texture: egui::TextureHandle, pixels: Vec<Color32>, last_frame: Instant, save_deadline: Instant }
impl App {
    fn new(cc:&eframe::CreationContext<'_>,rom:impl AsRef<Path>)->anyhow::Result<Self>{let save_dir=PathBuf::from("saves");let cart=Cartridge::load(rom,&save_dir)?;let gba=Gba::load(cart);let texture=cc.egui_ctx.load_texture("gba-frame",egui::ColorImage::new([WIDTH,HEIGHT],Color32::BLACK),TextureOptions::NEAREST);Ok(Self{gba,texture,pixels:vec![Color32::BLACK;WIDTH*HEIGHT],last_frame:Instant::now(),save_deadline:Instant::now()+Duration::from_millis(250)})}
    fn upload(&mut self,ctx:&egui::Context){for(dst,&p)in self.pixels.iter_mut().zip(self.gba.framebuffer()){let r=((p&31)<<3)as u8;let g=(((p>>5)&31)<<3)as u8;let b=(((p>>10)&31)<<3)as u8;*dst=Color32::from_rgb(r|r>>5,g|g>>5,b|b>>5);}let pixels=std::mem::replace(&mut self.pixels,vec![Color32::BLACK;WIDTH*HEIGHT]);self.texture.set(egui::ColorImage::new([WIDTH,HEIGHT],pixels),TextureOptions::NEAREST);ctx.request_repaint();}
}
impl eframe::App for App {
    fn update(&mut self,ctx:&egui::Context,_frame:&mut eframe::Frame){self.gba.run_frame();self.upload(ctx);if Instant::now()>=self.save_deadline{let _=self.gba.flush_save();self.save_deadline=Instant::now()+Duration::from_millis(250);}egui::CentralPanel::default().show(ctx,|ui|{ui.heading("gba-rust");ui.label(format!("{} · {:?}",self.gba.title(),self.gba.save_kind()));let a=ui.available_size();let s=(a.x/WIDTH as f32).min(a.y/HEIGHT as f32).max(1.0);ui.centered_and_justified(|ui|ui.image((self.texture.id(),egui::vec2(WIDTH as f32*s,HEIGHT as f32*s)));});let target=Duration::from_micros(16_667);let e=self.last_frame.elapsed();if e<target{ctx.request_repaint_after(target-e);}self.last_frame=Instant::now();}
    fn on_exit(&mut self,_gl:Option<&eframe::glow::Context>){let _=self.gba.flush_save();}
}
fn main()->eframe::Result{tracing_subscriber::fmt::init();let rom=std::env::args().nth(1).unwrap_or_else(||"roms/1636 - Pokemon Fire Red (U)(Squirrels).gba".into());let options=eframe::NativeOptions{viewport:egui::ViewportBuilder::default().with_inner_size([960.0,720.0]).with_min_inner_size([480.0,320.0]),..Default::default()};eframe::run_native("gba-rust",options,Box::new(move|cc|Ok(Box::new(App::new(cc,rom).expect("failed to load GBA ROM"))))) }
