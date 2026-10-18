#!/usr/bin/env python3
"""Prepare an exclusive replay directory; run the unchanged measurement script separately."""
import hashlib,json,pathlib,shutil,sys,tarfile
root=pathlib.Path(__file__).resolve().parent
base=root/'retained'
expected=json.loads((base/'inputs-v2.json').read_text())
pin=json.loads((base/'release-pin.json').read_text())
def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda:f.read(1048576),b''):h.update(block)
    return h.hexdigest()
archive=pathlib.Path(sys.argv[2]).resolve()
if digest(archive)!=pin['archive_sha256']:raise SystemExit('release archive hash mismatch')
for name,h in expected.items():
    if not name.startswith('release/') and digest(base/name)!=h:raise SystemExit('input hash mismatch: '+name)
out=pathlib.Path(sys.argv[1]).resolve()
if shutil.disk_usage(out.parent).free<2*1024**3:raise SystemExit('requires 2 GiB free')
out.mkdir(exist_ok=False)
for name in expected:
    if not name.startswith('release/'):shutil.copy2(base/name,out/name)
shutil.copy2(base/'inputs-v2.json',out/'inputs.json')
# Extract only the single pinned executable; no archive paths are used as destinations.
binary=out/'release/assay-v6.14.1-aarch64-unknown-linux-gnu/assay'
binary.parent.mkdir(parents=True)
with tarfile.open(archive,'r:gz') as t:
    member=t.getmember(pin['binary_member'])
    if not member.isfile() or member.size>100*1024**2:raise SystemExit('unexpected binary member')
    with t.extractfile(member) as src,binary.open('xb') as dst:shutil.copyfileobj(src,dst)
binary.chmod(0o755)
for name,h in expected.items():
    if digest(out/name)!=h:raise SystemExit('prepared input hash mismatch: '+name)
print('Prepared exact pinned inputs at',out)
print('On Linux aarch64 with Python 3 and GNU time, run:')
print('python3',out/'run_linux_v2.py')
