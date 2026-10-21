import pathlib
import plistlib
import unittest
from ios_entitlements import REQUIRED, validate

class Entitlements(unittest.TestCase):
    def test_both_build_routes(self):
        root = pathlib.Path(__file__).resolve().parent.parent
        for name in ['Muguet.entitlements', 'Muguet-sideload.entitlements']:
            validate((root / 'ios' / name).read_bytes())
        self.assertIn('CODE_SIGN_ENTITLEMENTS: Muguet-sideload.entitlements',
                      (root / 'ios/project.yml').read_text())

    def test_missing_false_and_wrong_types_rejected(self):
        for key in REQUIRED:
            for value in [None, False, 'true', 1]:
                values = dict.fromkeys(REQUIRED, True)
                if value is None:
                    del values[key]
                else:
                    values[key] = value
                with self.assertRaises(ValueError):
                    validate(plistlib.dumps(values))

if __name__ == '__main__':
    unittest.main()
