"""Package the actual linked Rust/AOT application; no game archives or shaders."""
import hashlib,json,os,pathlib,plistlib,shutil,subprocess,sys,zipfile
from ios_entitlements import validate, verify_signature
root=pathlib.Path(__file__).resolve().parent.parent
os.chdir(root)
binary=pathlib.Path(sys.argv[1]).resolve()
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
assert len(revision)==40 and all(c in '0123456789abcdef' for c in revision), 'invalid source revision'
app=root/'build/GTA-Rebirth.app'
assert not app.exists(), 'refuse to replace existing application'
app.mkdir(parents=True)
shutil.copyfile(binary,app/'muguet');os.chmod(app/'muguet',0o755)
subprocess.run(['node','tools/prepare-icons.mjs'],check=True)
subprocess.run(['xcrun','actool','build/Rebirth.xcassets','--compile',str(app),'--platform','iphoneos','--minimum-deployment-target','15.0','--app-icon','AppIcon','--target-device','iphone','--target-device','ipad','--output-partial-info-plist','build/icon-info.plist'],check=True)
info=plistlib.loads((root/'ios/Info.plist').read_bytes())
info.update(plistlib.loads((root/'build/icon-info.plist').read_bytes()))
info.update(CFBundleIdentifier='io.github.yosnuffyyy.gtanativeprobe',CFBundleExecutable='muguet',MinimumOSVersion='15.0',RebirthSourceRevision=revision)
assert info['CFBundleDisplayName']=='GTA V (Test Build)'
(app/'Info.plist').write_bytes(plistlib.dumps(info))
validate(pathlib.Path('ios/Muguet-sideload.entitlements').read_bytes())
subprocess.run(['codesign','--force','--sign','-','--entitlements','ios/Muguet-sideload.entitlements','--generate-entitlement-der',str(app)],check=True)
subprocess.run(['codesign','--verify','--strict',str(app)],check=True)
signed_entitlements = verify_signature(app)
(root/'evidence/local/signed-entitlements.json').write_text(json.dumps(signed_entitlements,indent=2)+'\n')
name=f"GTA-Rebirth-{info['CFBundleShortVersionString']}-{revision[:12]}-unsigned.ipa"
ipa=root/'build'/name
with zipfile.ZipFile(ipa,'x',compression=zipfile.ZIP_DEFLATED,compresslevel=6) as z:
 for p in sorted(app.rglob('*')):
  if p.is_file():z.write(p,'Payload/GTA-Rebirth.app/'+p.relative_to(app).as_posix())
with zipfile.ZipFile(ipa) as z:
 bundled=plistlib.loads(z.read('Payload/GTA-Rebirth.app/Info.plist'))
 assert all(bundled[key]==info[key] for key in ['CFBundleShortVersionString','CFBundleVersion','CFBundleIdentifier','RebirthSourceRevision']), 'packaged app identity mismatch'
evidence=dict(version=info['CFBundleShortVersionString'],build=info['CFBundleVersion'],bundle=info['CFBundleIdentifier'],source=revision,upstream='1b4600b052b864c78fe36180073ff406663b7e0b',ipa=name,bytes=ipa.stat().st_size,sha256=hashlib.file_digest(ipa.open('rb'),'sha256').hexdigest(),signing='ad-hoc; requires user signing for installation',phone_verified=False)
(root/'evidence/local/package.json').write_text(json.dumps(evidence,indent=2)+'\n')
subprocess.run(['node','tools/seal-build.mjs','seal',str(ipa),'tools/delivery-public.pem','build/rebirth-private.ipa.enc'],check=True)
print(json.dumps(evidence,indent=2))
