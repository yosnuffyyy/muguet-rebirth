// UI-only simulator executable. No game module, runtime or archive reader.
// Stubs cannot enter the production target (only tools/preview-ios.py uses it).
import SwiftUI
import UIKit
@_cdecl("rebirth_prepare") func previewPrepare(_ a:UnsafePointer<CChar>,_ b:UnsafePointer<CChar>,_ c:UnsafePointer<CChar>)->Bool {false}
@_cdecl("rebirth_error") func previewError()->UnsafePointer<CChar>? {nil}
@_cdecl("muguet_has_game_code") func previewHasCode()->Bool {false}
@_cdecl("rebirth_logs_start") func previewStartLogs(_ root:UnsafePointer<CChar>)->Bool {true}
@_cdecl("rebirth_logs_export") func previewExport(_ root:UnsafePointer<CChar>)->UnsafePointer<CChar>? {nil}
@main final class PreviewApplication:UIResponder,UIApplicationDelegate {
    var window:UIWindow?
    func application(_ application:UIApplication,didFinishLaunchingWithOptions options:[UIApplication.LaunchOptionsKey:Any]?=nil)->Bool {
        let page=CommandLine.arguments.dropFirst().first.flatMap(LibraryPage.init(rawValue:)) ?? .home
        let w=UIWindow(frame:UIScreen.main.bounds)
        w.rootViewController=UIHostingController(rootView:LauncherView(initialPage:page,play:{preconditionFailure("UI preview cannot run the game")}))
        w.makeKeyAndVisible();window=w;return true
    }
}
