"""Capture the actual Swift launcher on Simulator; explicitly no game runtime."""
import json,pathlib,plistlib,subprocess,time
def run(*args,**kw):return subprocess.run(args,check=True,**kw)
def out(*args):return subprocess.check_output(args,text=True).strip()
app=pathlib.Path('build/LauncherPreview.app');app.mkdir(parents=True,exist_ok=True)
sdk=out('xcrun','--sdk','iphonesimulator','--show-sdk-path')
run('xcrun','swiftc','-parse-as-library','-emit-executable','-O','-target','arm64-apple-ios15.0-simulator','-sdk',sdk,'ios/RebirthSettings.swift','ios/RebirthLauncher.swift','tests/LauncherPreview.swift','-o',str(app/'Preview'))
bundle='io.github.yosnuffyyy.rebirth.preview'
plist={'CFBundleIdentifier':bundle,'CFBundleExecutable':'Preview','CFBundleName':'Rebirth UI Preview','CFBundlePackageType':'APPL','CFBundleVersion':'1','CFBundleShortVersionString':'1','MinimumOSVersion':'15.0','UIDeviceFamily':[1],'UILaunchScreen':{},'UISupportedInterfaceOrientations':['UIInterfaceOrientationLandscapeLeft','UIInterfaceOrientationLandscapeRight']}
(app/'Info.plist').write_bytes(plistlib.dumps(plist))
runtimes=json.loads(out('xcrun','simctl','list','runtimes','--json'))['runtimes']
runtime=next(r['identifier'] for r in reversed(runtimes) if r.get('isAvailable') and 'iOS' in r['name'])
device=out('xcrun','simctl','create','RebirthLauncherPreview','com.apple.CoreSimulator.SimDeviceType.iPhone-16-Pro',runtime)
try:
 run('xcrun','simctl','boot',device)
 run('xcrun','simctl','bootstatus',device,'-b',timeout=150)
 run('xcrun','simctl','install',device,str(app))
 for page in ['Home','Graphics','Controls','Audio','Advanced','Game Files','Diagnostics']:
  run('xcrun','simctl','launch',device,bundle,page)
  time.sleep(2)
  run('xcrun','simctl','io',device,'screenshot',f'evidence/local/launcher-{page.lower().replace(" ","-")}.png')
  run('xcrun','simctl','terminate',device,bundle)
 pathlib.Path('evidence/local/ui-preview.json').write_text(json.dumps({'runtime':runtime,'device':device,'gameplay':False,'productionRustLinked':False,'purpose':'actual Swift launcher layout only'},indent=2))
finally:
 subprocess.run(['xcrun','simctl','shutdown',device],check=False)
 subprocess.run(['xcrun','simctl','delete',device],check=False)
