"""Check the actual signature, not just the input plist. No signing credentials."""
import plistlib
import subprocess

REQUIRED = (
    'com.apple.developer.kernel.increased-memory-limit',
    'com.apple.developer.kernel.extended-virtual-addressing',
)

def validate(data):
    values = plistlib.loads(data)
    for key in REQUIRED:
        if values.get(key) is not True:
            raise ValueError(f'Missing true entitlement: {key}')
    return values

def verify_signature(app):
    result = subprocess.run(['codesign', '-d', '--entitlements', ':-', str(app)],
                            check=True, capture_output=True)
    return validate(result.stdout)
