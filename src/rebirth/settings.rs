//! Requested configuration is validated once, then frozen before engine startup.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub schema: u32,
    pub mode: u32,
    pub preset: String,
    pub width: u32,
    pub height: u32,
    pub scale: u32,
    pub fps: u32,
    pub texture: u32,
    pub fxaa: bool,
    pub shadow: i32,
    pub reflection: u32,
    pub water: u32,
    pub particles: u32,
    pub grass: u32,
    pub postfx: u32,
    pub anisotropy: u32,
    pub ssao: u32,
    pub density: f64,
    pub volume: f64,
    pub mute: bool,
    pub touch_sensitivity: f64,
    pub controller_sensitivity: f64,
    pub show_fps: bool,
}
impl Default for Settings {
    fn default() -> Self { Self {
        schema: 1, mode: 1, preset: "Low".into(), width: 640, height: 360,
        scale: 100, fps: 30, texture: 0, fxaa: false, shadow: -1,
        reflection: 0, water: 0, particles: 0, grass: 0, postfx: 0,
        anisotropy: 0, ssao: 0, density: 1.0, volume: 1.0, mute: false,
        touch_sensitivity: 1.0, controller_sensitivity: 1.0, show_fps: true,
    } }
}
impl Settings {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= 16 * 1024, "settings exceed 16 KiB");
        let s: Self = serde_json::from_slice(bytes)?;
        s.validate()?;
        Ok(s)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported Rebirth settings schema");
        ensure!([1,2].contains(&self.mode), "select Story or Sandbox");
        ensure!(["Ultra Potato","Potato","Low","Balanced","Quality","Custom"].contains(&self.preset.as_str()), "invalid preset");
        // Muguet accepts explicit integer backbuffer dimensions. These are
        // conservative launch bounds, not a claim about all device budgets.
        ensure!((320..=2560).contains(&self.width) && (180..=1440).contains(&self.height), "base resolution outside validated bounds");
        ensure!(self.width > self.height && self.width % 2 == 0 && self.height % 2 == 0, "base resolution must be even landscape dimensions");
        ensure!([50,60,70,80,90,100].contains(&self.scale), "unsupported render scale");
        ensure!([15,20,30].contains(&self.fps), "only verified 60-Hz divisor caps 15/20/30 are available");
        ensure!(self.texture <= 2 && (-1..=2).contains(&self.shadow) && self.reflection <= 3 && self.water <= 1 && self.particles <= 2 && self.grass <= 3 && self.postfx <= 3 && self.ssao <= 2, "graphics value outside original engine domain");
        ensure!([0,2,4,8,16].contains(&self.anisotropy), "invalid anisotropic filtering");
        ensure!(self.density.is_finite() && (0.0..=1.0).contains(&self.density), "invalid ambient density");
        ensure!(self.volume.is_finite() && (0.0..=1.0).contains(&self.volume), "invalid master volume");
        for s in [self.touch_sensitivity,self.controller_sensitivity] { ensure!(s.is_finite() && (0.25..=2.0).contains(&s), "invalid look sensitivity"); }
        let (w,h) = self.resolution();
        ensure!(w >= 320 && h >= 180, "scaled resolution is below 320x180");
        Ok(())
    }
    pub fn resolution(&self) -> (u32,u32) {
        // Even dimensions; at most one output pixel rounding in either axis.
        ((self.width * self.scale + 100) / 200 * 2, (self.height * self.scale + 100) / 200 * 2)
    }
    pub fn engine_args(&self) -> Result<Vec<String>> {
        self.validate()?;
        let mut a = vec!["-nonetwork".into(),format!("-wasmstartchoice={}",self.mode),format!("-frameLimit={}",60/self.fps),"-FrameQueueLimit=2".into()];
        if self.mode == 1 { a.push("-noautoload".into()); }
        else { a.push("-playercoords=94.95,-742.96,44.75".into()); }
        for (name,value) in [
            ("textureQuality",self.texture as i32),("fxaa",self.fxaa as i32),
            ("shadowQuality",self.shadow),("reflectionQuality",self.reflection as i32),
            ("waterQuality",self.water as i32),("particleQuality",self.particles as i32),
            ("grassQuality",self.grass as i32),("postFX",self.postfx as i32),
            ("anisotropicQualityLevel",self.anisotropy as i32),("SSAO",self.ssao as i32),
            ("shaderQuality",1),("multiSample",1),("multiSampleQuality",0),("tessellation",0),("reflectionMSAA",0),("txaa",0),
        ] { a.push(format!("-{name}={value}")); }
        a.push(format!("-cityDensity={}", self.density));
        Ok(a)
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn scaling_and_modes() {
        let mut s=Settings::default();
        for (scale,w,h) in [(50,320,180),(70,448,252),(100,640,360)] {
            s.scale=scale; s.validate().unwrap(); assert_eq!(s.resolution(),(w,h));
        }
        for mode in [1,2,1,2] { s.mode=mode; let a=s.engine_args().unwrap();
            assert_eq!(a.contains(&"-noautoload".into()),mode==1);
            assert_eq!(a.iter().any(|v|v.starts_with("-playercoords=")),mode==2);
        }
        for (fps,div) in [(15,4),(20,3),(30,2)] {s.fps=fps;assert!(s.engine_args().unwrap().contains(&format!("-frameLimit={div}")));}
    }
    #[test] fn rejects_unsupported_and_roundtrips() {
        let s=Settings::default(); assert_eq!(s,Settings::parse(&serde_json::to_vec(&s).unwrap()).unwrap());
        for fps in [0,24,40,60] {let mut s=s.clone();s.fps=fps;assert!(s.validate().is_err());}
        let mut s=s;s.volume=f64::NAN;assert!(s.validate().is_err());
        assert!(Settings::parse(br#"{"rendererV2":true}"#).is_err());
        assert!(Settings::parse(br#"{"mode":3}"#).is_err());
    }
}
