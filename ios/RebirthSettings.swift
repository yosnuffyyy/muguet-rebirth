// Project Rebirth customization; upstream Muguet remains the runtime.
import Foundation
import CoreFoundation

// Installed identity comes from the signed bundle, never a hardcoded UI label.
struct RebirthBuildIdentity {
    let version: String, build: String, source: String, bundle: String
    init(info: [String: Any]) {
        func value(_ key: String) -> String {
            guard let raw = info[key] as? String else { return "Unknown" }
            let text = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !text.isEmpty, text.utf8.count <= 128, !text.contains("$("),
                  !text.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains) else { return "Unknown" }
            return text
        }
        version = value("CFBundleShortVersionString")
        build = value("CFBundleVersion")
        let revision = value("RebirthSourceRevision").lowercased()
        source = revision.count == 40 && revision.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) } ? revision : "Unknown"
        bundle = value("CFBundleIdentifier")
    }
    var summary: String { "Rebirth \(version) · Build \(build)" }
    var diagnostic: String { "installed app: Rebirth version=\(version) build=\(build) source=\(source) bundle=\(bundle)" }
}

struct RebirthSettings: Codable, Equatable {
    var schema = 1, mode = 1
    var preset = "Low"
    var width = 640, height = 360, scale = 100, fps = 30, texture = 0
    var fxaa = false
    var shadow = -1, reflection = 0, water = 0, particles = 0, grass = 0, postfx = 0, anisotropy = 0, ssao = 0
    var density = 1.0, volume = 1.0
    var mute = false
    var touch_sensitivity = 1.0, controller_sensitivity = 1.0
    var show_fps = true
    func validate() throws {
        func require(_ condition:Bool) throws {if !condition {throw CocoaError(.coderReadCorrupt)}}
        try require(schema == 1 && [1,2].contains(mode) && Self.presets.contains(preset))
        try require((320...2560).contains(width) && (180...1440).contains(height) && width > height && width%2 == 0 && height%2 == 0)
        try require([50,60,70,80,90,100].contains(scale) && [15,20,30,60].contains(fps))
        try require((width*scale+100)/200*2 >= 320 && (height*scale+100)/200*2 >= 180)
        try require((0...2).contains(texture) && (-1...2).contains(shadow) && (0...3).contains(reflection) && (0...1).contains(water) && (0...2).contains(particles) && (0...3).contains(grass) && (0...3).contains(postfx) && (0...2).contains(ssao) && [0,2,4,8,16].contains(anisotropy))
        try require(density.isFinite && (0...1).contains(density) && volume.isFinite && (0...1).contains(volume))
        try require(touch_sensitivity.isFinite && (0.25...2).contains(touch_sensitivity) && controller_sensitivity.isFinite && (0.25...2).contains(controller_sensitivity))
    }
    static let presets = ["Ultra Potato", "Potato", "Low", "Balanced", "Quality", "Custom"]
    var resolution: String { "\((width * scale + 100) / 200 * 2) × \((height * scale + 100) / 200 * 2)" }
    mutating func applyPreset(_ name: String) {
        guard Self.presets.contains(name) else { return }
        preset = name
        guard name != "Custom" else { return }
        let keepMode = mode, keepVolume = volume, keepMute = mute
        let touch = touch_sensitivity, controller = controller_sensitivity, overlay = show_fps
        self = Self(); mode = keepMode; volume = keepVolume; mute = keepMute
        touch_sensitivity = touch; controller_sensitivity = controller; show_fps = overlay; preset = name
        switch name {
        case "Ultra Potato": scale = 50; density = 0.25
        case "Potato": scale = 70; density = 0.5
        case "Balanced": fxaa = true; shadow = 0; reflection = 1; postfx = 1; anisotropy = 4
        case "Quality": width = 960; height = 540; texture = 1; fxaa = true; shadow = 1; reflection = 2; postfx = 2; anisotropy = 8
        default: break
        }
    }
    static func migrateLegacy(_ values: [String: Any]) -> Self {
        // Known, compatible values only; no renderer/core flags are transplanted.
        var s = Self()
        func integer(_ key: String, _ allowed: ClosedRange<Int>, _ fallback: Int) -> Int {
            guard let n = values[key] as? NSNumber, CFGetTypeID(n) != CFBooleanGetTypeID(), n.doubleValue.rounded() == n.doubleValue, allowed.contains(n.intValue) else { return fallback }
            return n.intValue
        }
        s.mode = integer("mode",1...2,1)
        let bases = [(640,360),(960,540),(1280,720)]
        let b = integer("base",0...2,0); (s.width,s.height) = bases[b]
        let scale = integer("scale",50...100,100); s.scale = [50,60,70,80,90,100].contains(scale) ? scale : 100
        let fps = integer("fps",15...60,30); s.fps = [15,20,30,60].contains(fps) ? fps : 30
        s.texture = integer("texture",0...2,0); s.fxaa = integer("aa",0...1,0) == 1
        s.shadow = integer("shadow",(-1)...2,-1); s.reflection = integer("reflection",0...3,0)
        s.water = integer("water",0...1,0); s.particles = integer("particles",0...2,0)
        s.grass = integer("grass",0...3,0); s.postfx = integer("postfx",0...3,0)
        let af = integer("anisotropy",0...16,0); s.anisotropy = [0,2,4,8,16].contains(af) ? af : 0
        s.ssao = integer("ssao",0...2,0)
        func real(_ key:String,_ range:ClosedRange<Double>,_ fallback:Double)->Double {
            guard let v=values[key] as? NSNumber,CFGetTypeID(v) != CFBooleanGetTypeID(),v.doubleValue.isFinite,range.contains(v.doubleValue) else{return fallback};return v.doubleValue
        }
        s.density=real("cityDensity",0...1,1); s.volume=real("volume",0...1,1)
        s.touch_sensitivity=real("touchSensitivity",0.25...2,1); s.controller_sensitivity=real("controllerSensitivity",0.25...2,1)
        s.mute=values["mute"] as? Bool ?? false;s.show_fps=values["showFPS"] as? Bool ?? true
        s.preset="Custom"; return s
    }
}

enum RebirthStore {
    static let key="rebirth.settings.v1"
    static func load(_ defaults:UserDefaults = .standard) throws -> RebirthSettings {
        if let data=defaults.data(forKey:key) {guard data.count <= 16384 else{throw CocoaError(.coderReadCorrupt)};let s=try JSONDecoder().decode(RebirthSettings.self,from:data);try s.validate();return s}
        if let legacy=defaults.dictionary(forKey:"launcherSettings.v1") {
            if defaults.object(forKey:"rebirth.legacySettings.backup.v1") == nil { defaults.set(legacy,forKey:"rebirth.legacySettings.backup.v1") }
            let s=RebirthSettings.migrateLegacy(legacy);try save(s,defaults);return s
        }
        return RebirthSettings()
    }
    static func save(_ s:RebirthSettings,_ defaults:UserDefaults = .standard) throws {try s.validate();defaults.set(try JSONEncoder().encode(s),forKey:key)}
}
