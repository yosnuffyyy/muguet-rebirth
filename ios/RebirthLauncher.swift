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

// MARK: - Visual language
// Near-black ink, one warm accent, hairline rules and square-ish corners.
// Motion is one-shot (no repeat-forever loops) and honours Reduce Motion.
private enum Theme {
    static let ink=Color(red:0.043,green:0.047,blue:0.055)
    static let panel=Color(red:0.086,green:0.092,blue:0.104)
    static let line=Color.white.opacity(0.09)
    static let muted=Color.white.opacity(0.6)
    static let gold=Color(red:0.97,green:0.76,blue:0.20)
}

private struct PanelStyle:ViewModifier {
    func body(content:Content)->some View {
        content.background(Theme.panel,in:RoundedRectangle(cornerRadius:6))
            .overlay(RoundedRectangle(cornerRadius:6).stroke(Theme.line,lineWidth:1))
    }
}
private extension View {
    func panel()->some View {modifier(PanelStyle())}
    func reveal(_ delay:Double)->some View {modifier(RevealStyle(delay:delay))}
}

private struct RevealStyle:ViewModifier {
    let delay:Double
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown=false
    func body(content:Content)->some View {
        content.opacity(shown || reduceMotion ? 1:0).offset(y:shown || reduceMotion ? 0:14)
            .onAppear{withAnimation(.easeOut(duration:0.45).delay(delay)){shown=true}}
    }
}

private struct PressStyle:ButtonStyle {
    func makeBody(configuration:Configuration)->some View {
        configuration.label.scaleEffect(configuration.isPressed ? 0.96:1).opacity(configuration.isPressed ? 0.85:1)
            .animation(.easeOut(duration:0.12),value:configuration.isPressed)
    }
}

private struct SectionHeader:View {
    let index:String
    let title:String
    var body:some View {
        HStack(spacing:10) {
            Text(index).font(.caption2.monospacedDigit().weight(.bold)).foregroundStyle(Theme.gold)
            Text(title.uppercased()).font(.caption.weight(.bold)).kerning(1.8).foregroundStyle(Theme.muted)
            Rectangle().fill(Theme.line).frame(height:1)
        }
        .accessibilityElement(children:.combine).accessibilityAddTraits(.isHeader)
    }
}

// MARK: - Mode artwork
// Artwork is looked up by name in the app's asset catalog (see tools/prepare-icons.mjs).
// Builds without the catalog, such as the UI-test harness, draw original fallback art.
private struct FallbackArt:View {
    let mode:Int
    var body:some View {
        ZStack(alignment:.bottom) {
            LinearGradient(colors:mode==1 ? [Color(red:0.36,green:0.22,blue:0.14),Color(red:0.08,green:0.07,blue:0.07)] : [Color(red:0.20,green:0.42,blue:0.52),Color(red:0.07,green:0.12,blue:0.15)],startPoint:.top,endPoint:.bottom)
            HStack(alignment:.bottom,spacing:4) {ForEach(0..<24,id:\.self){i in Rectangle().fill(Color.black.opacity(0.28)).frame(height:CGFloat(30+(i*37)%90))}}.padding(.horizontal,12)
        }
    }
}
private struct ModeArtwork:View {
    let mode:Int
    var body:some View {
        if let image=UIImage(named:mode==1 ? "ModeStory":"ModeSandbox") {
            Image(uiImage:image).resizable().scaledToFill()
        } else {
            FallbackArt(mode:mode)
        }
    }
}
// Slow one-shot push-in. Re-created per mode (via .id) so each picture drifts from its start.
private struct DriftingArtwork:View {
    let mode:Int
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var drifted=false
    var body:some View {
        ModeArtwork(mode:mode)
            .scaleEffect(drifted && !reduceMotion ? 1.1:1,anchor:mode==1 ? .topLeading:.bottomTrailing)
            .animation(reduceMotion ? nil:.linear(duration:30),value:drifted)
            .onAppear{drifted=true}
    }
}
private struct ModeHero:View {
    let mode:Int
    let height:CGFloat
    var body:some View {
        ZStack(alignment:.bottomLeading) {
            DriftingArtwork(mode:mode).id(mode)
                .frame(maxWidth:.infinity,maxHeight:.infinity).clipped()
                .transition(.opacity)
            LinearGradient(colors:[.clear,Color.black.opacity(0.82)],startPoint:.center,endPoint:.bottom)
            LinearGradient(colors:[Color.black.opacity(0.55),.clear],startPoint:.leading,endPoint:.center)
            VStack(alignment:.leading,spacing:6) {
                Text(mode==1 ? "THE ORIGINAL CAMPAIGN":"OFFLINE OPEN WORLD").font(.caption2.weight(.bold)).kerning(2).foregroundStyle(Theme.gold)
                Text(mode==1 ? "Story Mode":"Sandbox Mode").font(.largeTitle.weight(.heavy)).foregroundStyle(.white)
                Text(mode==1 ? "Three lives, one city. Progress is saved separately." : "Free roam with its own saves. No story required.").font(.subheadline).foregroundStyle(Color.white.opacity(0.78)).fixedSize(horizontal:false,vertical:true)
            }
            .padding(20).id("hero-text-\(mode)")
            .transition(.asymmetric(insertion:AnyTransition.opacity.combined(with:.offset(x:0,y:12)),removal:.opacity))
        }
        .frame(height:height).frame(maxWidth:.infinity)
        .clipShape(RoundedRectangle(cornerRadius:6))
        .overlay(RoundedRectangle(cornerRadius:6).stroke(Theme.line,lineWidth:1))
        .animation(.easeInOut(duration:0.6),value:mode)
        .accessibilityElement(children:.combine)
    }
}
private struct ModeSwitch:View {
    @Binding var mode:Int
    @Namespace private var pill
    private let items:[(Int,String)]=[(1,"Story"),(2,"Sandbox")]
    var body:some View {
        HStack(spacing:4) {
            ForEach(items,id:\.0) {item in
                Button {
                    guard mode != item.0 else{return}
                    UISelectionFeedbackGenerator().selectionChanged()
                    withAnimation(.spring(response:0.4,dampingFraction:0.82)){mode=item.0}
                } label: {
                    Text(item.1.uppercased()).font(.footnote.weight(.bold)).kerning(1.6)
                        .foregroundStyle(mode==item.0 ? Color.black:Theme.muted)
                        .frame(maxWidth:.infinity,minHeight:44)
                        .background {
                            if mode==item.0 {RoundedRectangle(cornerRadius:4).fill(Theme.gold).matchedGeometryEffect(id:"pill",in:pill)}
                        }
                        .contentShape(Rectangle())
                }
                .buttonStyle(PressStyle())
                .accessibilityAddTraits(mode==item.0 ? .isSelected:[])
            }
        }
        .padding(4).panel()
        .accessibilityElement(children:.contain).accessibilityLabel("Game mode")
    }
}

// MARK: - Settings rows
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
        .frame(minHeight:44).padding(.vertical,4).padding(.horizontal,14)
        .overlay(alignment:.bottom) {Rectangle().fill(Theme.line).frame(height:1).padding(.leading,14).accessibilityHidden(true)}
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
                    Text(currentTitle).multilineTextAlignment(.trailing).foregroundStyle(Theme.gold)
                        .fixedSize(horizontal:false,vertical:true)
                    Image(systemName:"chevron.up.chevron.down").font(.caption).foregroundStyle(Theme.muted)
                        .accessibilityHidden(true)
                }
                .frame(maxWidth:.infinity,minHeight:44,alignment:.trailing)
                .padding(.horizontal,12).padding(.vertical,4)
                .background(Theme.ink,in:RoundedRectangle(cornerRadius:4))
                .overlay(RoundedRectangle(cornerRadius:4).stroke(Theme.line,lineWidth:1))
                .contentShape(Rectangle())
            }
            .accessibilityIdentifier("graphics.\(id).control")
            .accessibilityLabel(title).accessibilityValue(currentTitle)
            .onChange(of:selection){_ in UISelectionFeedbackGenerator().selectionChanged()}
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
    @Namespace private var navMark
    @ObservedObject private var folder=SelectedFolder.shared
    let play:()->Void
    init(initialPage:LibraryPage = .home,play:@escaping()->Void){_page=State(initialValue:initialPage);self.play=play}
    private let gold=Theme.gold
    private var canPlay:Bool {loaded && folder.url != nil && hasGameCode()}
    var body:some View {
        GeometryReader { geometry in
            VStack(spacing:0) {
                if typeSize.isAccessibilitySize {
                    Menu {ForEach(LibraryPage.allCases,id:\.self){item in Button(item.rawValue){select(item)}}} label:{Label(page.rawValue,systemImage:page.icon).frame(minHeight:44)}
                        .accessibilityLabel("Launcher section").padding(.horizontal,16)
                } else if geometry.size.width < 660 {ScrollView(.horizontal,showsIndicators:false){HStack{navigation}}.padding(10)}
                HStack(spacing:0) {
                    if geometry.size.width >= 660 && !typeSize.isAccessibilitySize {
                        ScrollView {VStack(alignment:.leading,spacing:5){
                            VStack(alignment:.leading,spacing:2){Text("REBIRTH").font(.headline.weight(.heavy)).kerning(3).foregroundStyle(gold);Rectangle().fill(gold).frame(width:28,height:2)}.padding(.bottom,16)
                            navigation;Spacer()
                            Text("MUGUET FORK\nGTA V (Test Build)").font(.caption2).foregroundStyle(Theme.muted)
                        }.padding(16)}
                        .frame(width:172).background(Color.white.opacity(0.03))
                        .overlay(alignment:.trailing){Rectangle().fill(Theme.line).frame(width:1)}
                    }
                    ZStack {
                        ScrollView {VStack(alignment:.leading,spacing:18){
                            VStack(alignment:.leading,spacing:6){
                                Rectangle().fill(gold).frame(width:28,height:2).accessibilityHidden(true)
                                Text(page.rawValue).font(.largeTitle.weight(.heavy))
                                Text(Self.installed.summary).font(.caption.monospaced()).foregroundStyle(Theme.muted).fixedSize(horizontal:false,vertical:true).accessibilityIdentifier("rebirth.installedVersion")
                            }
                            if let error {Text(error).foregroundStyle(.red).fixedSize(horizontal:false,vertical:true)}
                            content
                        }.padding(22).frame(maxWidth:900,alignment:.leading)}
                        .accessibilityIdentifier("launcher.content")
                        .id(page)
                        .transition(.asymmetric(insertion:AnyTransition.opacity.combined(with:.offset(x:18,y:0)),removal:.opacity))
                    }.frame(maxWidth:.infinity,maxHeight:.infinity)
                }
                Rectangle().fill(LinearGradient(colors:[gold.opacity(0.55),Theme.line,.clear],startPoint:.leading,endPoint:.trailing)).frame(height:1)
                HStack(spacing:16){
                    VStack(alignment:.leading,spacing:2){
                        Text(s.mode==1 ? "STORY MODE" : "SANDBOX MODE").font(.caption.weight(.bold)).kerning(1.4).foregroundStyle(gold)
                        Text("\(s.preset) · \(s.resolution) · \(s.fps) FPS cap").font(.caption).foregroundStyle(Theme.muted).fixedSize(horizontal:false,vertical:true)
                    }
                    Spacer()
                    Button{UIImpactFeedbackGenerator(style:.medium).impactOccurred();start()} label:{
                        HStack(spacing:10){
                            Text(starting ? "PREPARING":"PLAY").font(.headline.weight(.heavy)).kerning(2)
                            Image(systemName:starting ? "hourglass":"play.fill").accessibilityHidden(true)
                        }
                        .foregroundStyle(.black).padding(.horizontal,32).frame(minHeight:48).background(gold,in:RoundedRectangle(cornerRadius:4))
                    }
                    .buttonStyle(PressStyle()).disabled(!canPlay).opacity(canPlay ? 1:0.45)
                    .animation(.easeInOut(duration:0.3),value:canPlay)
                }.padding(.horizontal,22).padding(.vertical,12)
            }.background(Theme.ink.ignoresSafeArea()).tint(gold).preferredColorScheme(.dark)
        }
        .onAppear{do{let support=FileManager.default.urls(for:.applicationSupportDirectory,in:.userDomainMask)[0];let logRoot=support.appendingPathComponent("RebirthLogs");if !logRoot.path.withCString({startLogs($0)}){error=launchError().map{String(cString:$0)}}else{Self.installed.diagnostic.withCString{identifyLogs($0)}};s=try RebirthStore.load();loaded=true}catch{self.error="Saved settings could not be read. Reset settings to recover."}}
        .onChange(of:s){value in guard loaded else{return};do{try RebirthStore.save(value)}catch{self.error=error.localizedDescription}}
        .sheet(item:$sharedLog){log in LogShare(url:log.url)}
        .disabled(starting)
        .fileImporter(isPresented:$picker,allowedContentTypes:[.folder]){result in do{try folder.select(result.get());error=nil}catch{self.error=error.localizedDescription}}
    }
    private func select(_ item:LibraryPage) {
        guard item != page else{return}
        UISelectionFeedbackGenerator().selectionChanged()
        withAnimation(.easeInOut(duration:0.3)){page=item}
    }
    @ViewBuilder private var navigation:some View {
        ForEach(LibraryPage.allCases,id:\.self){item in
            Button{select(item)} label:{
                HStack(spacing:10){
                    Image(systemName:item.icon).frame(width:20).accessibilityHidden(true)
                    Text(item.rawValue).font(.subheadline.weight(page==item ? .semibold:.regular)).lineLimit(1)
                }
                .frame(maxWidth:.infinity,alignment:.leading).padding(.horizontal,12).frame(minHeight:44)
                .background {
                    if page==item {
                        ZStack(alignment:.leading){RoundedRectangle(cornerRadius:4).fill(gold.opacity(0.12));Rectangle().fill(gold).frame(width:3)}
                            .matchedGeometryEffect(id:"navMark",in:navMark)
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(PressStyle()).foregroundStyle(page==item ? gold:Color.white.opacity(0.7))
            .accessibilityAddTraits(page==item ? .isSelected:[])
        }
    }
    private var heroHeight:CGFloat {typeSize.isAccessibilitySize ? 170:210}
    @ViewBuilder private var content:some View {
        switch page {
        case .home,.game:
            ModeHero(mode:s.mode,height:heroHeight).reveal(0.0)
            ModeSwitch(mode:$s.mode).reveal(0.08)
            card("Ready your next session",s.mode==1 ? "Original Story startup with its own saved progress." : "Original offline Sandbox startup, with separate saved progress.").reveal(0.16)
            folderCard.reveal(0.24)
            if !hasGameCode(){card("Runtime validation build","This build has no embedded game module. PLAY remains unavailable; a compiled launcher is not proof of gameplay.").reveal(0.32)}
            card("Runtime","Muguet · Wasmtime / Cranelift · Rust wgpu / Metal. Previous native renderers and CPU cores are not linked into this fork.").reveal(0.4)
        case .graphics:
            settingsGroup("01","Preset & resolution") {
                LauncherSettingPicker(title:"Graphics Preset",id:"preset",selection:Binding(get:{s.preset},set:{s.applyPreset($0)}),options:RebirthSettings.presets.map{($0,$0)})
                LauncherSettingPicker(title:"Resolution",id:"resolution",selection:Binding(get:{"\(s.width)x\(s.height)"},set:{let v=$0.split(separator:"x");s.width=Int(v[0])!;s.height=Int(v[1])!;s.preset="Custom"}),options:["640x360","854x480","960x540","1280x720"].map{($0,$0.replacingOccurrences(of:"x",with:" × "))},savedTitle:"\(s.width) × \(s.height)")
                choice("Resolution Scale","scale",\RebirthSettings.scale,[(50,"50%"),(70,"70%"),(100,"100%")],savedTitle:"\(s.scale)%")
            }
            Text("Internal rendering: \(s.resolution). Display and touch coordinates remain independent.").font(.caption).foregroundStyle(Theme.muted)
            settingsGroup("02","Performance") {
                choice("Frame Rate Limit","fps",\RebirthSettings.fps,[(15,"15 FPS"),(20,"20 FPS"),(30,"30 FPS"),(60,"60 FPS")])
            }
            settingsGroup("03","Image quality") {
                choice("Texture Quality","texture",\RebirthSettings.texture,[(0,"Normal"),(1,"High"),(2,"Very High")])
                settingToggle("FXAA","fxaa",edit(\RebirthSettings.fxaa))
                choice("Shadow Quality","shadow",\RebirthSettings.shadow,[(-1,"Off"),(0,"Normal"),(1,"High"),(2,"Very High")])
                quality("Reflection Quality","reflection",\RebirthSettings.reflection,3);quality("Water Quality","water",\RebirthSettings.water,1);quality("Particle Quality","particles",\RebirthSettings.particles,2);quality("Grass Quality","grass",\RebirthSettings.grass,3);quality("Post Processing","postfx",\RebirthSettings.postfx,3);quality("Ambient Occlusion","ssao",\RebirthSettings.ssao,2)
                choice("Anisotropic Filtering","anisotropy",\RebirthSettings.anisotropy,[(0,"Off"),(2,"2×"),(4,"4×"),(8,"8×"),(16,"16×")])
            }
            settingsGroup("04","Overlay") {
                settingToggle("Frame Statistics","statistics",$s.show_fps)
            }
            card("Unavailable pending validation","MSAA, frame rates above 60 or uncapped rendering, native-display resolution and dynamic upscaling are not enabled. All graphics changes apply at the next clean launch.")
            Color.clear.frame(height:64).accessibilityHidden(true)
        case .gameplay:slider("Ambient population",edit(\RebirthSettings.density),0...1);card("Separate progression","Story and Sandbox receive separate byte-verified Rebirth save copies. Legacy saves remain untouched for rollback. No save payload conversion is performed.")
        case .controls:slider("Touch look sensitivity",$s.touch_sensitivity,0.25...2);slider("Controller look sensitivity",$s.controller_sensitivity,0.25...2);card("Muguet controls","Native GameController and touch input remain upstream-owned. Additional deadzone, layout, invert-axis and switching preferences need integration testing before being exposed.")
        case .audio:slider("Master volume",$s.volume,0...1);Toggle("Mute",isOn:$s.mute).padding(14).panel();card("Native audio","Gain is applied to actual 48 kHz stereo PCM at the CoreAudio output. Muting still drains the ring. Dialogue/radio/effects bus controls are unavailable. Speaker playback requires device verification.")
        case .files:folderCard;card("Files stay where they are","Select your existing web folder. Archives and shader data are read in place; no ZIP import, download or replacement. The launcher validates the pinned original shader build before starting.")
        case .advanced:card("Execution architecture","Muguet Rust host, Wasmtime AOT and wgpu 30 Metal. This fork does not contain selectable legacy wasm2c, Renderer V2 or CPU Core V2 paths.");card("Offline","Multiplayer is disabled for this migration milestone.");Button("Reset Settings"){s=RebirthSettings();loaded=true;error=nil;try? RebirthStore.save(s)}.buttonStyle(.bordered)
        case .diagnostics:card(Self.installed.summary,"Source: \(Self.installed.source)\nApp: \(Self.installed.bundle)");Toggle("Display diagnosis (temporary magenta test)",isOn:$displayProbe).padding(14).panel();Text("On next launch: 30 test presents, then eight tiny pixel samples. Off by default; does not prove gameplay or visible output.").font(.caption).foregroundStyle(Theme.muted);card("Validation status","Source and host tests do not prove iPhone gameplay. Exact upstream/source revisions and validation results accompany each build.");card("Session reports","Share a TXT snapshot of Muguet's engine and runtime logs. Retention is bounded at 256 MiB across launches; older text is rotated. Original game assets are excluded.");Button(exporting ? "Preparing report…" : "Share TXT Report",action:shareLogs).buttonStyle(.bordered).disabled(exporting);Text("Upstream 1b4600b · Rebirth development milestone").font(.caption.monospaced()).foregroundStyle(Theme.muted)
        }
    }
    private var folderCard:some View{VStack(alignment:.leading,spacing:8){Text("GAME FILES").font(.caption.weight(.bold)).kerning(1.8).foregroundStyle(gold);Text(folder.url?.lastPathComponent ?? "No folder selected").font(.headline);if let e=folder.error{Text(e).foregroundStyle(.red)};Text("Permission restored does not imply all assets are present.").font(.caption).foregroundStyle(Theme.muted);Button("Select / Change Folder"){picker=true}.buttonStyle(.bordered)}.frame(maxWidth:.infinity,alignment:.leading).padding(16).panel()}
    private func card(_ title:String,_ text:String)->some View{VStack(alignment:.leading,spacing:7){Text(title).font(.headline);Text(text).font(.subheadline).foregroundStyle(Theme.muted).fixedSize(horizontal:false,vertical:true)}.frame(maxWidth:.infinity,alignment:.leading).padding(16).panel()}
    private func settingsGroup<Rows:View>(_ index:String,_ title:String,@ViewBuilder _ rows:()->Rows)->some View {
        VStack(alignment:.leading,spacing:10) {
            SectionHeader(index:index,title:title)
            VStack(spacing:0){rows()}.panel()
        }
    }
    private func edit<T>(_ key:WritableKeyPath<RebirthSettings,T>)->Binding<T>{Binding(get:{s[keyPath:key]},set:{s[keyPath:key]=$0;s.preset="Custom"})}
    private func choice(_ title:String,_ id:String,_ key:WritableKeyPath<RebirthSettings,Int>,_ values:[(Int,String)],savedTitle:String?=nil)->some View{LauncherSettingPicker(title:title,id:id,selection:edit(key),options:values,savedTitle:savedTitle)}
    private func quality(_ title:String,_ id:String,_ key:WritableKeyPath<RebirthSettings,Int>,_ maximum:Int)->some View{choice(title,id,key,(0...maximum).map{($0,["Normal","High","Very High","Ultra"][$0])})}
    private func settingToggle(_ title:String,_ id:String,_ value:Binding<Bool>)->some View {
        LauncherSettingRow(title:title,id:id) {
            HStack(spacing:8) {
                Text(value.wrappedValue ? "On":"Off").foregroundStyle(value.wrappedValue ? gold:Theme.muted).fixedSize(horizontal:false,vertical:true)
                Toggle(title,isOn:value).labelsHidden().fixedSize()
                    .accessibilityLabel(title).accessibilityIdentifier("graphics.\(id).control")
            }
        }
    }
    private func slider(_ title:String,_ value:Binding<Double>,_ range:ClosedRange<Double>)->some View{VStack(alignment:.leading,spacing:8){HStack{Text(title);Spacer();Text(String(format:"%.2f",value.wrappedValue)).font(.body.monospacedDigit()).foregroundStyle(gold)};Slider(value:value,in:range)}.padding(14).panel()}
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
