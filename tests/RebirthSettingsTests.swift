import Foundation
@main struct SettingsTests {
    static func main() throws {
        var checks=0
        func check(_ condition:Bool) {precondition(condition);checks+=1}
        var s=RebirthSettings()
        for (scale,resolution) in [(50,"320 × 180"),(70,"448 × 252"),(100,"640 × 360")] {
            s.scale=scale;try s.validate();check(s.resolution==resolution)
        }
        let suite="rebirth-test-\(UUID().uuidString)"
        let defaults=UserDefaults(suiteName:suite)!
        defer {defaults.removePersistentDomain(forName:suite)}
        defaults.set(["mode":2,"base":0,"scale":70,"fps":20,"volume":0.25,"renderer":1],forKey:"launcherSettings.v1")
        s=try RebirthStore.load(defaults)
        check(s.mode==2 && s.scale==70 && s.fps==20 && s.volume==0.25)
        check(defaults.dictionary(forKey:"rebirth.legacySettings.backup.v1") != nil)
        for name in RebirthSettings.presets {
            s.applyPreset(name);try s.validate();check(s.preset==name && s.mode==2 && s.volume==0.25)
            try RebirthStore.save(s,defaults);check(try RebirthStore.load(defaults)==s)
        }
        s.fps=60
        do {try RebirthStore.save(s,defaults);preconditionFailure("unsupported cap accepted")}catch{checks+=1}
        s=RebirthSettings.migrateLegacy(["width":Int.max,"fps":true,"volume":Double.nan,"mode":3,"scale":72])
        try s.validate();check(s.fps==30 && s.volume==1 && s.mode==1 && s.scale==100)
        defaults.set(Data("{bad".utf8),forKey:RebirthStore.key)
        do {_=try RebirthStore.load(defaults);preconditionFailure("corrupt config hidden")}catch{checks+=1}
        print("PASS: \(checks) Swift settings, migration, preset and persistence assertions")
    }
}
