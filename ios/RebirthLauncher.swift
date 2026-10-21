// Original library composition; no proprietary launcher assets.
import SwiftUI
import UIKit
import UniformTypeIdentifiers

@_silgen_name("rebirth_prepare") private func prepareLaunch(_ root:UnsafePointer<CChar>,_ json:UnsafePointer<CChar>,_ saves:UnsafePointer<CChar>)->Bool
@_silgen_name("rebirth_error") private func launchError()->UnsafePointer<CChar>?
@_silgen_name("muguet_has_game_code") private func hasGameCode()->Bool

final class SelectedFolder: ObservableObject {
    static let shared=SelectedFolder()
    @Published var url:URL?
    @Published var error:String?
    private var scoped=false
    init() {
        guard let data=UserDefaults.standard.data(forKey:"gameFolderBookmark") else{return}
        do {
            var stale=false
            let restored=try URL(resolvingBookmarkData:data,options:[],relativeTo:nil,bookmarkDataIsStale:&stale)
            try select(restored,save:stale)
        } catch {self.error="Folder access needs renewal: \(error.localizedDescription)"}
    }
    func select(_ next:URL,save:Bool=true)throws {
        let access=next.startAccessingSecurityScopedResource()
        do {
            guard try next.resourceValues(forKeys:[.isDirectoryKey]).isDirectory == true else{throw CocoaError(.fileReadUnsupportedScheme)}
            if save {let bookmark=try next.bookmarkData(options:[],includingResourceValuesForKeys:nil,relativeTo:nil);UserDefaults.standard.set(bookmark,forKey:"gameFolderBookmark")}
        } catch {if access{next.stopAccessingSecurityScopedResource()};throw error}
        if scoped{url?.stopAccessingSecurityScopedResource()}
        url=next;scoped=access;error=nil
    }
    // Held for the entire engine session. Upstream exits the process on stop;
    // never release an active archive scope when the launcher is dismissed.
    deinit {if scoped{url?.stopAccessingSecurityScopedResource()}}
}
enum LibraryPage:String,CaseIterable {case home="Home",game="GTA V",graphics="Graphics",gameplay="Gameplay",controls="Controls",audio="Audio",advanced="Advanced",files="Game Files",diagnostics="Diagnostics"
    var icon:String{switch self{case .home:return "square.grid.2x2";case .game:return "gamecontroller";case .graphics:return "display";case .gameplay:return "person.fill";case .controls:return "dpad";case .audio:return "speaker.wave.2";case .advanced:return "slider.horizontal.3";case .files:return "folder";case .diagnostics:return "waveform.path.ecg"}}
}
struct LauncherView:View {
    @State private var s=RebirthSettings()
    @State private var page=LibraryPage.home
    @State private var picker=false
    @State private var error:String?
    @State private var loaded=false
    @ObservedObject private var folder=SelectedFolder.shared
    let play:()->Void
    private let gold=Color(red:0.97,green:0.76,blue:0.20)
    var body:some View {
        GeometryReader { geometry in
            VStack(spacing:0) {
                if geometry.size.width < 660 {ScrollView(.horizontal,showsIndicators:false){HStack{navigation}}.padding(10)}
                HStack(spacing:0) {
                    if geometry.size.width >= 660 {VStack(alignment:.leading,spacing:5){Text("REBIRTH").font(.headline).foregroundStyle(gold).padding(.bottom,14);navigation;Spacer();Text("MUGUET FORK\nGTA V (Test Build)").font(.caption2).foregroundStyle(.secondary)}.padding(16).frame(width:172).background(Color.white.opacity(0.03))}
                    ScrollView {VStack(alignment:.leading,spacing:18){
                        Text(page.rawValue).font(.title2.bold())
                        if let error {Text(error).foregroundStyle(.red).fixedSize(horizontal:false,vertical:true)}
                        content
                    }.padding(22).frame(maxWidth:900,alignment:.leading)}
                }
                Divider()
                HStack(spacing:16){VStack(alignment:.leading){Text(s.mode==1 ? "STORY MODE" : "SANDBOX MODE").font(.caption.bold());Text("\(s.preset) · \(s.resolution) · \(s.fps) FPS cap").font(.caption).foregroundStyle(.secondary)};Spacer();Button(action:start){Label("PLAY",systemImage:"play.fill").font(.headline).foregroundStyle(.black).padding(.horizontal,35).frame(minHeight:48).background(gold,in:RoundedRectangle(cornerRadius:10))}.buttonStyle(.plain).disabled(!loaded || folder.url==nil || !hasGameCode()).opacity(!loaded || folder.url==nil || !hasGameCode() ? 0.45:1)}.padding(.horizontal,22).padding(.vertical,12)
            }.background(Color(red:0.055,green:0.061,blue:0.068)).tint(gold).preferredColorScheme(.dark)
        }
        .onAppear{do{s=try RebirthStore.load();loaded=true}catch{self.error="Saved settings could not be read. Reset settings to recover."}}
        .onChange(of:s){value in guard loaded else{return};do{try RebirthStore.save(value)}catch{self.error=error.localizedDescription}}
        .fileImporter(isPresented:$picker,allowedContentTypes:[.folder]){result in do{try folder.select(result.get());error=nil}catch{self.error=error.localizedDescription}}
    }
    @ViewBuilder private var navigation:some View {ForEach(LibraryPage.allCases,id:\.self){item in Button{page=item} label:{Label(item.rawValue,systemImage:item.icon).font(.subheadline.weight(page==item ? .semibold:.regular)).frame(maxWidth:.infinity,alignment:.leading).padding(11).background(page==item ? gold.opacity(0.13):.clear,in:RoundedRectangle(cornerRadius:8))}.buttonStyle(.plain).foregroundStyle(page==item ? gold:Color.white.opacity(0.75))}}
    @ViewBuilder private var content:some View {
        switch page {
        case .home,.game:
            Picker("Game mode",selection:$s.mode){Text("Story Mode").tag(1);Text("Sandbox Mode").tag(2)}.pickerStyle(.segmented)
            ZStack(alignment:.bottomLeading){LinearGradient(colors:[Color(red:0.16,green:0.19,blue:0.23),.black],startPoint:.topLeading,endPoint:.bottomTrailing);HStack(alignment:.bottom,spacing:8){ForEach(0..<20,id:\.self){i in Rectangle().fill(Color.white.opacity(0.035)).frame(width:CGFloat(12+i%4*7),height:CGFloat(22+(i*37)%80))}}.padding(.leading,130);VStack(alignment:.leading,spacing:6){Text("LOS SANTOS / YOUR LIBRARY").font(.caption2.bold()).foregroundStyle(gold);Text("GRAND THEFT AUTO V").font(.title.bold());Text("GTA V (Test Build)").font(.subheadline).foregroundStyle(.secondary)}.padding(24)}.frame(height:150).clipShape(RoundedRectangle(cornerRadius:14))
            card("Ready your next session",s.mode==1 ? "Original Story startup with its own saved progress." : "Original offline Sandbox startup, with separate saved progress.")
            folderCard
            if !hasGameCode(){card("Runtime validation build","This build has no embedded game module. PLAY remains unavailable; a compiled launcher is not proof of gameplay.")}
            card("Runtime","Muguet · Wasmtime / Cranelift · Rust wgpu / Metal. Previous native renderers and CPU cores are not linked into this fork.")
        case .graphics:
            Picker("Preset",selection:Binding(get:{s.preset},set:{s.applyPreset($0)})){ForEach(RebirthSettings.presets,id:\.self){Text($0).tag($0)}}
            Picker("Base resolution",selection:Binding(get:{"\(s.width)x\(s.height)"},set:{let v=$0.split(separator:"x");s.width=Int(v[0])!;s.height=Int(v[1])!;s.preset="Custom"})){ForEach(["640x360","854x480","960x540","1280x720"],id:\.self){Text($0).tag($0)}}
            choice("Render scale",\RebirthSettings.scale,[(50,"50%"),(70,"70%"),(100,"100%")])
            Text("Internal rendering: \(s.resolution). Display and touch coordinates remain independent.").font(.caption).foregroundStyle(.secondary)
            choice("Frame cap",\RebirthSettings.fps,[(15,"15 FPS"),(20,"20 FPS"),(30,"30 FPS")])
            choice("Textures",\RebirthSettings.texture,[(0,"Normal"),(1,"High"),(2,"Very High")])
            Toggle("FXAA",isOn:edit(\RebirthSettings.fxaa))
            choice("Shadows",\RebirthSettings.shadow,[(-1,"Off"),(0,"Normal"),(1,"High"),(2,"Very High")])
            quality("Reflections",\RebirthSettings.reflection,3);quality("Water",\RebirthSettings.water,1);quality("Particles",\RebirthSettings.particles,2);quality("Grass",\RebirthSettings.grass,3);quality("Post processing",\RebirthSettings.postfx,3);quality("Ambient occlusion",\RebirthSettings.ssao,2)
            choice("Anisotropic filtering",\RebirthSettings.anisotropy,[(0,"Off"),(2,"2×"),(4,"4×"),(8,"8×"),(16,"16×")])
            Toggle("Frame statistics",isOn:$s.show_fps)
            card("Unavailable pending validation","MSAA, higher or uncapped frame rates, native-display resolution and dynamic upscaling are not enabled. All graphics changes apply at the next clean launch.")
        case .gameplay:slider("Ambient population",edit(\RebirthSettings.density),0...1);card("Separate progression","Story and Sandbox receive separate byte-verified Rebirth save copies. Legacy saves remain untouched for rollback. No save payload conversion is performed.")
        case .controls:slider("Touch look sensitivity",$s.touch_sensitivity,0.25...2);slider("Controller look sensitivity",$s.controller_sensitivity,0.25...2);card("Muguet controls","Native GameController and touch input remain upstream-owned. Additional deadzone, layout, invert-axis and switching preferences need integration testing before being exposed.")
        case .audio:slider("Master volume",$s.volume,0...1);Toggle("Mute",isOn:$s.mute);card("Native audio","Gain is applied to actual 48 kHz stereo PCM at the CoreAudio output. Muting still drains the ring. Dialogue/radio/effects bus controls are unavailable. Speaker playback requires device verification.")
        case .files:folderCard;card("Files stay where they are","Select your existing web folder. Archives and shader data are read in place; no ZIP import, download or replacement. The launcher validates the pinned original shader build before starting.")
        case .advanced:card("Execution architecture","Muguet Rust host, Wasmtime AOT and wgpu 30 Metal. This fork does not contain selectable legacy wasm2c, Renderer V2 or CPU Core V2 paths.");card("Offline","Multiplayer is disabled for this migration milestone.");Button("Reset Settings"){s=RebirthSettings();loaded=true;error=nil;try? RebirthStore.save(s)}.buttonStyle(.bordered)
        case .diagnostics:card("Validation status","Source and host tests do not prove iPhone gameplay. Exact upstream/source revisions and validation results accompany each build.");card("Session reports","Muguet logging remains active. Persistent shared TXT reports and previous-session recovery are not yet integrated into this fork.");Text("Upstream 1b4600b · Rebirth development milestone").font(.caption.monospaced())
        }
    }
    private var folderCard:some View{VStack(alignment:.leading,spacing:8){Text("GAME FILES").font(.caption.bold()).foregroundStyle(gold);Text(folder.url?.lastPathComponent ?? "No folder selected").font(.headline);if let e=folder.error{Text(e).foregroundStyle(.red)};Text("Permission restored does not imply all assets are present.").font(.caption).foregroundStyle(.secondary);Button("Select / Change Folder"){picker=true}.buttonStyle(.bordered)}.frame(maxWidth:.infinity,alignment:.leading).padding(16).background(Color.white.opacity(0.04),in:RoundedRectangle(cornerRadius:12))}
    private func card(_ title:String,_ text:String)->some View{VStack(alignment:.leading,spacing:7){Text(title).font(.headline);Text(text).font(.subheadline).foregroundStyle(.secondary).fixedSize(horizontal:false,vertical:true)}.frame(maxWidth:.infinity,alignment:.leading).padding(16).background(Color.white.opacity(0.04),in:RoundedRectangle(cornerRadius:12))}
    private func edit<T>(_ key:WritableKeyPath<RebirthSettings,T>)->Binding<T>{Binding(get:{s[keyPath:key]},set:{s[keyPath:key]=$0;s.preset="Custom"})}
    private func choice(_ title:String,_ key:WritableKeyPath<RebirthSettings,Int>,_ values:[(Int,String)])->some View{Picker(title,selection:edit(key)){ForEach(values,id:\.0){Text($0.1).tag($0.0)}}}
    private func quality(_ title:String,_ key:WritableKeyPath<RebirthSettings,Int>,_ maximum:Int)->some View{choice(title,key,(0...maximum).map{($0,["Normal","High","Very High","Ultra"][$0])})}
    private func slider(_ title:String,_ value:Binding<Double>,_ range:ClosedRange<Double>)->some View{VStack(alignment:.leading){HStack{Text(title);Spacer();Text(String(format:"%.2f",value.wrappedValue)).monospacedDigit()};Slider(value:value,in:range)}}
    private func start(){
        guard loaded,let root=folder.url,hasGameCode() else{return}
        do {
            try RebirthStore.save(s)
            let json=String(decoding:try JSONEncoder().encode(s),as:UTF8.self)
            let support=FileManager.default.urls(for:.applicationSupportDirectory,in:.userDomainMask)[0]
            try FileManager.default.createDirectory(at:support,withIntermediateDirectories:true)
            let ok=root.path.withCString{r in json.withCString{j in support.path.withCString{p in prepareLaunch(r,j,p)}}}
            guard ok else{error=launchError().map{String(cString:$0)} ?? "Launch validation failed";return}
            play()
        }catch{self.error=error.localizedDescription}
    }
}
