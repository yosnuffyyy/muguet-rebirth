// Original library composition; no proprietary launcher assets.
import SwiftUI
import UIKit
import UniformTypeIdentifiers

@_silgen_name("rebirth_prepare") private func prepareLaunch(_ root:UnsafePointer<CChar>,_ json:UnsafePointer<CChar>,_ saves:UnsafePointer<CChar>)->Bool
@_silgen_name("rebirth_error") private func launchError()->UnsafePointer<CChar>?
@_silgen_name("rebirth_logs_start") private func startLogs(_ root:UnsafePointer<CChar>)->Bool
@_silgen_name("rebirth_logs_identify") private func identifyLogs(_ identity:UnsafePointer<CChar>)
@_silgen_name("rebirth_logs_export") private func exportLogs(_ root:UnsafePointer<CChar>)->UnsafePointer<CChar>?
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

// Picker's menu presentation does not render its title in a plain VStack.
// Own the label and both columns here, independent of the control's style.
private struct LauncherSettingRow<Control:View>:View {
    let title:String
    let id:String
    @ViewBuilder let control:()->Control
    var body:some View {
        HStack(alignment:.center,spacing:16) {
            Text(title).font(.body).foregroundStyle(.primary)
                .fixedSize(horizontal:false,vertical:true)
                .frame(maxWidth:.infinity,alignment:.leading)
                .accessibilityIdentifier("graphics.\(id).label")
            control().frame(maxWidth:.infinity,alignment:.trailing)
        }
        .frame(minHeight:44).padding(.vertical,4)
        .accessibilityElement(children:.contain)
    }
}

private struct LauncherSettingPicker<Value:Hashable>:View {
    let title:String
    let id:String
    @Binding var selection:Value
    let options:[(Value,String)]
    var savedTitle:String? = nil
    private var currentTitle:String {
        options.first(where:{$0.0==selection})?.1 ?? savedTitle ?? String(describing:selection)
    }
    var body:some View {
        LauncherSettingRow(title:title,id:id) {
            Menu {
                Picker(title,selection:$selection) {
                    // Preserve valid migrated/custom values absent from the quick choices.
                    if !options.contains(where:{$0.0==selection}) {Text(currentTitle).tag(selection)}
                    ForEach(options,id:\.0) {Text($0.1).tag($0.0)}
                }.pickerStyle(.inline)
            } label: {
                HStack(spacing:8) {
                    Text(currentTitle).multilineTextAlignment(.trailing)
                        .fixedSize(horizontal:false,vertical:true)
                    Image(systemName:"chevron.up.chevron.down").font(.caption)
                        .accessibilityHidden(true)
                }
                .frame(maxWidth:.infinity,minHeight:44,alignment:.trailing)
                .padding(.horizontal,12).padding(.vertical,4)
                .background(Color.white.opacity(0.05),in:RoundedRectangle(cornerRadius:8))
                .contentShape(Rectangle())
            }
            .accessibilityIdentifier("graphics.\(id).control")
            .accessibilityLabel(title).accessibilityValue(currentTitle)
        }
    }
}

struct LauncherView:View {
    private static let installed = RebirthBuildIdentity(info: Bundle.main.infoDictionary ?? [:])
    @Environment(\.dynamicTypeSize) private var typeSize
    @AppStorage("rebirth.displayProbe") private var displayProbe=false
    @State private var s=RebirthSettings()
    @State private var page=LibraryPage.home
    @State private var picker=false
    @State private var error:String?
    @State private var loaded=false
    @State private var starting=false
    @State private var exporting=false
    @State private var sharedLog:SharedLog?
    @ObservedObject private var folder=SelectedFolder.shared
    let play:()->Void
    init(initialPage:LibraryPage = .home,play:@escaping()->Void){_page=State(initialValue:initialPage);self.play=play}
    private let gold=Color(red:0.97,green:0.76,blue:0.20)
    var body:some View {
        GeometryReader { geometry in
            VStack(spacing:0) {
                if typeSize.isAccessibilitySize {
                    Menu {ForEach(LibraryPage.allCases,id:\.self){item in Button(item.rawValue){page=item}}} label:{Label(page.rawValue,systemImage:page.icon).frame(minHeight:44)}
                        .accessibilityLabel("Launcher section").padding(.horizontal,16)
                } else if geometry.size.width < 660 {ScrollView(.horizontal,showsIndicators:false){HStack{navigation}}.padding(10)}
                HStack(spacing:0) {
                    if geometry.size.width >= 660 && !typeSize.isAccessibilitySize {ScrollView {VStack(alignment:.leading,spacing:5){Text("REBIRTH").font(.headline).foregroundStyle(gold).padding(.bottom,14);navigation;Spacer();Text("MUGUET FORK\nGTA V (Test Build)").font(.caption2).foregroundStyle(.secondary)}.padding(16)}.frame(width:172).background(Color.white.opacity(0.03))}
                    ScrollView {VStack(alignment:.leading,spacing:18){
                        VStack(alignment:.leading,spacing:4){Text(page.rawValue).font(.title2.bold());Text(Self.installed.summary).font(.caption.monospaced()).foregroundStyle(.secondary).fixedSize(horizontal:false,vertical:true).accessibilityIdentifier("rebirth.installedVersion")}
                        if let error {Text(error).foregroundStyle(.red).fixedSize(horizontal:false,vertical:true)}
                        content
                    }.padding(22).frame(maxWidth:900,alignment:.leading)}.accessibilityIdentifier("launcher.content")
                }
                Divider()
                HStack(spacing:16){VStack(alignment:.leading){Text(s.mode==1 ? "STORY MODE" : "SANDBOX MODE").font(.caption.bold());Text("\(s.preset) · \(s.resolution) · \(s.fps) FPS cap").font(.caption).foregroundStyle(.secondary)};Spacer();Button(action:start){Label(starting ? "PREPARING" : "PLAY",systemImage:"play.fill").font(.headline).foregroundStyle(.black).padding(.horizontal,35).frame(minHeight:48).background(gold,in:RoundedRectangle(cornerRadius:10))}.buttonStyle(.plain).disabled(!loaded || folder.url==nil || !hasGameCode()).opacity(!loaded || folder.url==nil || !hasGameCode() ? 0.45:1)}.padding(.horizontal,22).padding(.vertical,12)
            }.background(Color(red:0.055,green:0.061,blue:0.068)).tint(gold).preferredColorScheme(.dark)
        }
        .onAppear{do{let support=FileManager.default.urls(for:.applicationSupportDirectory,in:.userDomainMask)[0];let logRoot=support.appendingPathComponent("RebirthLogs");if !logRoot.path.withCString({startLogs($0)}){error=launchError().map{String(cString:$0)}}else{Self.installed.diagnostic.withCString{identifyLogs($0)}};s=try RebirthStore.load();loaded=true}catch{self.error="Saved settings could not be read. Reset settings to recover."}}
        .onChange(of:s){value in guard loaded else{return};do{try RebirthStore.save(value)}catch{self.error=error.localizedDescription}}
        .sheet(item:$sharedLog){log in LogShare(url:log.url)}
        .disabled(starting)
        .fileImporter(isPresented:$picker,allowedContentTypes:[.folder]){result in do{try folder.select(result.get());error=nil}catch{self.error=error.localizedDescription}}
    }
    @ViewBuilder private var navigation:some View {ForEach(LibraryPage.allCases,id:\.self){item in Button{page=item} label:{Label(item.rawValue,systemImage:item.icon).font(.subheadline.weight(page==item ? .semibold:.regular)).frame(maxWidth:.infinity,alignment:.leading).padding(11).background(page==item ? gold.opacity(0.13):.clear,in:RoundedRectangle(cornerRadius:8))}.buttonStyle(.plain).foregroundStyle(page==item ? gold:Color.white.opacity(0.75))}}
    @ViewBuilder private var content:some View {
        switch page {
        case .home,.game:
            Picker("Game mode",selection:$s.mode){Text("Story Mode").tag(1);Text("Sandbox Mode").tag(2)}.pickerStyle(.segmented)
            ZStack(alignment:.bottomLeading){LinearGradient(colors:[Color(red:0.16,green:0.19,blue:0.23),.black],startPoint:.topLeading,endPoint:.bottomTrailing);GeometryReader { art in HStack(alignment:.bottom,spacing:5){ForEach(0..<20,id:\.self){i in Rectangle().fill(Color.white.opacity(0.035)).frame(height:CGFloat(22+(i*37)%80))}}.frame(height:art.size.height,alignment:.bottom)}.padding(.leading,130);VStack(alignment:.leading,spacing:6){Text("LOS SANTOS / YOUR LIBRARY").font(.caption2.bold()).foregroundStyle(gold);Text("GRAND THEFT AUTO V").font(.title.bold());Text("GTA V (Test Build)").font(.subheadline).foregroundStyle(.secondary)}.padding(24)}.frame(height:150).clipShape(RoundedRectangle(cornerRadius:14))
            card("Ready your next session",s.mode==1 ? "Original Story startup with its own saved progress." : "Original offline Sandbox startup, with separate saved progress.")
            folderCard
            if !hasGameCode(){card("Runtime validation build","This build has no embedded game module. PLAY remains unavailable; a compiled launcher is not proof of gameplay.")}
            card("Runtime","Muguet · Wasmtime / Cranelift · Rust wgpu / Metal. Previous native renderers and CPU cores are not linked into this fork.")
        case .graphics:
            LauncherSettingPicker(title:"Graphics Preset",id:"preset",selection:Binding(get:{s.preset},set:{s.applyPreset($0)}),options:RebirthSettings.presets.map{($0,$0)})
            LauncherSettingPicker(title:"Resolution",id:"resolution",selection:Binding(get:{"\(s.width)x\(s.height)"},set:{let v=$0.split(separator:"x");s.width=Int(v[0])!;s.height=Int(v[1])!;s.preset="Custom"}),options:["640x360","854x480","960x540","1280x720"].map{($0,$0.replacingOccurrences(of:"x",with:" × "))},savedTitle:"\(s.width) × \(s.height)")
            choice("Resolution Scale","scale",\RebirthSettings.scale,[(50,"50%"),(70,"70%"),(100,"100%")],savedTitle:"\(s.scale)%")
            Text("Internal rendering: \(s.resolution). Display and touch coordinates remain independent.").font(.caption).foregroundStyle(.secondary)
            choice("Frame Rate Limit","fps",\RebirthSettings.fps,[(15,"15 FPS"),(20,"20 FPS"),(30,"30 FPS"),(60,"60 FPS")])
            choice("Texture Quality","texture",\RebirthSettings.texture,[(0,"Normal"),(1,"High"),(2,"Very High")])
            settingToggle("FXAA","fxaa",edit(\RebirthSettings.fxaa))
            choice("Shadow Quality","shadow",\RebirthSettings.shadow,[(-1,"Off"),(0,"Normal"),(1,"High"),(2,"Very High")])
            quality("Reflection Quality","reflection",\RebirthSettings.reflection,3);quality("Water Quality","water",\RebirthSettings.water,1);quality("Particle Quality","particles",\RebirthSettings.particles,2);quality("Grass Quality","grass",\RebirthSettings.grass,3);quality("Post Processing","postfx",\RebirthSettings.postfx,3);quality("Ambient Occlusion","ssao",\RebirthSettings.ssao,2)
            choice("Anisotropic Filtering","anisotropy",\RebirthSettings.anisotropy,[(0,"Off"),(2,"2×"),(4,"4×"),(8,"8×"),(16,"16×")])
            settingToggle("Frame Statistics","statistics",$s.show_fps)
            card("Unavailable pending validation","MSAA, frame rates above 60 or uncapped rendering, native-display resolution and dynamic upscaling are not enabled. All graphics changes apply at the next clean launch.")
        case .gameplay:slider("Ambient population",edit(\RebirthSettings.density),0...1);card("Separate progression","Story and Sandbox receive separate byte-verified Rebirth save copies. Legacy saves remain untouched for rollback. No save payload conversion is performed.")
        case .controls:slider("Touch look sensitivity",$s.touch_sensitivity,0.25...2);slider("Controller look sensitivity",$s.controller_sensitivity,0.25...2);card("Muguet controls","Native GameController and touch input remain upstream-owned. Additional deadzone, layout, invert-axis and switching preferences need integration testing before being exposed.")
        case .audio:slider("Master volume",$s.volume,0...1);Toggle("Mute",isOn:$s.mute);card("Native audio","Gain is applied to actual 48 kHz stereo PCM at the CoreAudio output. Muting still drains the ring. Dialogue/radio/effects bus controls are unavailable. Speaker playback requires device verification.")
        case .files:folderCard;card("Files stay where they are","Select your existing web folder. Archives and shader data are read in place; no ZIP import, download or replacement. The launcher validates the pinned original shader build before starting.")
        case .advanced:card("Execution architecture","Muguet Rust host, Wasmtime AOT and wgpu 30 Metal. This fork does not contain selectable legacy wasm2c, Renderer V2 or CPU Core V2 paths.");card("Offline","Multiplayer is disabled for this migration milestone.");Button("Reset Settings"){s=RebirthSettings();loaded=true;error=nil;try? RebirthStore.save(s)}.buttonStyle(.bordered)
        case .diagnostics:card(Self.installed.summary,"Source: \(Self.installed.source)\nApp: \(Self.installed.bundle)");Toggle("Display diagnosis (temporary magenta test)",isOn:$displayProbe);Text("On next launch: 30 test presents, then eight tiny pixel samples. Off by default; does not prove gameplay or visible output.").font(.caption);card("Validation status","Source and host tests do not prove iPhone gameplay. Exact upstream/source revisions and validation results accompany each build.");card("Session reports","Share a TXT snapshot of Muguet's engine and runtime logs. Retention is bounded at 256 MiB across launches; older text is rotated. Original game assets are excluded.");Button(exporting ? "Preparing report…" : "Share TXT Report",action:shareLogs).buttonStyle(.bordered).disabled(exporting);Text("Upstream 1b4600b · Rebirth development milestone").font(.caption.monospaced())
        }
    }
    private var folderCard:some View{VStack(alignment:.leading,spacing:8){Text("GAME FILES").font(.caption.bold()).foregroundStyle(gold);Text(folder.url?.lastPathComponent ?? "No folder selected").font(.headline);if let e=folder.error{Text(e).foregroundStyle(.red)};Text("Permission restored does not imply all assets are present.").font(.caption).foregroundStyle(.secondary);Button("Select / Change Folder"){picker=true}.buttonStyle(.bordered)}.frame(maxWidth:.infinity,alignment:.leading).padding(16).background(Color.white.opacity(0.04),in:RoundedRectangle(cornerRadius:12))}
    private func card(_ title:String,_ text:String)->some View{VStack(alignment:.leading,spacing:7){Text(title).font(.headline);Text(text).font(.subheadline).foregroundStyle(.secondary).fixedSize(horizontal:false,vertical:true)}.frame(maxWidth:.infinity,alignment:.leading).padding(16).background(Color.white.opacity(0.04),in:RoundedRectangle(cornerRadius:12))}
    private func edit<T>(_ key:WritableKeyPath<RebirthSettings,T>)->Binding<T>{Binding(get:{s[keyPath:key]},set:{s[keyPath:key]=$0;s.preset="Custom"})}
    private func choice(_ title:String,_ id:String,_ key:WritableKeyPath<RebirthSettings,Int>,_ values:[(Int,String)],savedTitle:String?=nil)->some View{LauncherSettingPicker(title:title,id:id,selection:edit(key),options:values,savedTitle:savedTitle)}
    private func quality(_ title:String,_ id:String,_ key:WritableKeyPath<RebirthSettings,Int>,_ maximum:Int)->some View{choice(title,id,key,(0...maximum).map{($0,["Normal","High","Very High","Ultra"][$0])})}
    private func settingToggle(_ title:String,_ id:String,_ value:Binding<Bool>)->some View {
        LauncherSettingRow(title:title,id:id) {
            HStack(spacing:8) {
                Text(value.wrappedValue ? "On":"Off").fixedSize(horizontal:false,vertical:true)
                Toggle(title,isOn:value).labelsHidden().fixedSize()
                    .accessibilityLabel(title).accessibilityIdentifier("graphics.\(id).control")
            }
        }
    }
    private func slider(_ title:String,_ value:Binding<Double>,_ range:ClosedRange<Double>)->some View{VStack(alignment:.leading){HStack{Text(title);Spacer();Text(String(format:"%.2f",value.wrappedValue)).monospacedDigit()};Slider(value:value,in:range)}}
    private func shareLogs(){
        guard !exporting else{return};exporting=true
        let root=FileManager.default.urls(for:.applicationSupportDirectory,in:.userDomainMask)[0].appendingPathComponent("RebirthExports").path
        DispatchQueue.global(qos:.utility).async {
            let path=root.withCString{exportLogs($0)}.map{String(cString:$0)}
            let message=path == nil ? launchError().map{String(cString:$0)} : nil
            DispatchQueue.main.async {exporting=false;if let path{sharedLog=SharedLog(url:URL(fileURLWithPath:path))}else{error=message ?? "Report export failed"}}
        }
    }
    private func start(){
        guard loaded,let root=folder.url,hasGameCode() else{return}
        do {
            try RebirthStore.save(s)
            let json=String(decoding:try JSONEncoder().encode(s),as:UTF8.self)
            let support=FileManager.default.urls(for:.applicationSupportDirectory,in:.userDomainMask)[0]
            try FileManager.default.createDirectory(at:support,withIntermediateDirectories:true)
            starting=true
            let rootPath=root.path,supportPath=support.path
            DispatchQueue.global(qos:.userInitiated).async {
                let ok=rootPath.withCString{r in json.withCString{j in supportPath.withCString{p in prepareLaunch(r,j,p)}}}
                let message=ok ? nil : (launchError().map{String(cString:$0)} ?? "Launch validation failed")
                DispatchQueue.main.async {starting=false;if ok{play()}else{error=message}}
            }
        }catch{self.error=error.localizedDescription}
    }
}

private struct SharedLog:Identifiable {let id=UUID();let url:URL}
private struct LogShare:UIViewControllerRepresentable {
    let url:URL
    func makeUIViewController(context:Context)->UIActivityViewController {UIActivityViewController(activityItems:[url],applicationActivities:nil)}
    func updateUIViewController(_ controller:UIActivityViewController,context:Context){}
}
