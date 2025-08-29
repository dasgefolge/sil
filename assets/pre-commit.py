import sys

import platform
import subprocess

against = sys.argv[1]

old_cargo_toml = subprocess.run(['git', 'show', f'{against}:Cargo.toml'], stdout=subprocess.PIPE, encoding='utf-8', check=True).stdout
for old_line in old_cargo_toml.splitlines():
    if old_line.startswith('version = '):
        break
else:
    sys.exit('Missing version number in old Cargo.toml')
with open('Cargo.toml', encoding='utf-8') as f: #TODO check staged changes instead of worktree
    for new_line in f:
        if new_line.startswith('version = '):
            break
    else:
        sys.exit('Missing version number in Cargo.toml')
if old_line.strip() == new_line.strip():
    sys.exit('Missing version bump in Cargo.toml')

old_cargo_lock = subprocess.run(['git', 'show', f'{against}:Cargo.lock'], stdout=subprocess.PIPE, encoding='utf-8', check=True).stdout
with open('Cargo.lock', encoding='utf-8') as f: #TODO check staged changes instead of worktree
    new_cargo_lock = f.read()
if old_cargo_lock == new_cargo_lock: #TODO more precisely compare the version field
    sys.exit('Missing version bump in Cargo.lock')

subprocess.run(['cargo', 'check'], check=True)
if platform.system() == 'Windows':
    subprocess.run(['wsl', 'rsync', '--delete', '-av', '/mnt/c/Users/fenhl/git/github.com/dasgefolge/sil/stage/', '/home/fenhl/wslgit/github.com/dasgefolge/sil/', '--exclude', 'target'], check=True) # copy the tree to the WSL file system to improve compile times
    subprocess.run(['wsl', 'env', '-C', '/home/fenhl/wslgit/github.com/dasgefolge/sil', 'cargo', 'check'], check=True)
    subprocess.run(['wsl', '-d', 'nixos-m2', 'nix', 'build', '--no-link'], check=True)
