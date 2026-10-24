// UI-only simulator executable. No game module, runtime or archive reader.
// Stubs cannot enter the production target (only tools/preview-ios.py uses it).
import SwiftUI
import UIKit
@_cdecl("rebirth_prepare") func previewPrepare(_ a:UnsafePointer<CChar>,_ b:UnsafePointer<CChar>,_ c:UnsafePointer<CChar>)->Bool {false}
@_cdecl("rebirth_error") func previewError()->UnsafePointer<CChar>? {nil}
@_cdecl("muguet_has_game_code") func previewHasCode()->Bool {false}
@_cdecl("rebirth_logs_start") func previewStartLogs(_ root:UnsafePointer<CChar>)->Bool {true}
@_cdecl("rebirth_logs_identify") func previewIdentifyLogs(_ identity:UnsafePointer<CChar>) {}
@_cdecl("rebirth_logs_export") func previewExport(_ root:UnsafePointer<CChar>)->UnsafePointer<CChar>? {nil}
@main final class PreviewApplication:UIResponder,UIApplicationDelegate {
    var window:UIWindow?
    func application(_ application:UIApplication,didFinishLaunchingWithOptions options:[UIApplication.LaunchOptionsKey:Any]?=nil)->Bool {
        let page=CommandLine.arguments.dropFirst().first.flatMap(LibraryPage.init(rawValue:)) ?? .home
        // Fixture seeding is compiled only into this UI preview, using the real store.
        if CommandLine.arguments.contains("--fixture=high") {
            var settings=RebirthSettings();settings.preset="Custom"
            settings.width=1280;settings.height=720;settings.texture=2;settings.shadow=1
            settings.reflection=0;settings.fxaa=true;settings.anisotropy=8
            try! RebirthStore.save(settings)
        } else if CommandLine.arguments.contains("--fixture=legacy") {
            var settings=RebirthSettings();settings.preset="Custom"
            settings.width=854;settings.height=480;settings.scale=60
            try! RebirthStore.save(settings)
        }
        let type:DynamicTypeSize = CommandLine.arguments.contains("--type=accessibility5") ? .accessibility5 : CommandLine.arguments.contains("--type=xxxLarge") ? .xxxLarge : .large
        let w=UIWindow(frame:UIScreen.main.bounds)
        w.rootViewController=UIHostingController(rootView:LauncherView(initialPage:page,play:{preconditionFailure("UI preview cannot run the game")}).environment(\.dynamicTypeSize,type))
        w.makeKeyAndVisible();window=w;return true
    }
}
