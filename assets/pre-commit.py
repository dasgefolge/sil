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
new_cargo_toml = subprocess.run(['git', 'show', ':Cargo.toml'], stdout=subprocess.PIPE, encoding='utf-8', check=True).stdout
for new_line in new_cargo_toml.splitlines():
    if new_line.startswith('version = '):
        break
else:
    sys.exit('Missing version number in staged Cargo.toml')
if old_line.strip() == new_line.strip():
    sys.exit('Missing version bump in Cargo.toml')

old_cargo_lock = subprocess.run(['git', 'show', f'{against}:Cargo.lock'], stdout=subprocess.PIPE, encoding='utf-8', check=True).stdout
new_cargo_lock = subprocess.run(['git', 'show', ':Cargo.lock'], stdout=subprocess.PIPE, encoding='utf-8', check=True).stdout
if old_cargo_lock == new_cargo_lock: #TODO more precisely compare the version field
    sys.exit('Missing version bump in Cargo.lock')

subprocess.run(['cargo', 'check'], check=True)
if platform.system() == 'Windows':
    subprocess.run(['wsl', '-d', 'ubuntu-m2', 'rsync', '--mkpath', '--delete', '-av', '/mnt/c/Users/fenhl/git/github.com/dasgefolge/sil/stage/', '/home/fenhl/wslgit/github.com/dasgefolge/sil/', '--exclude', 'target'], check=True) # copy the tree to the WSL file system to improve compile times
    subprocess.run(['wsl', '-d', 'ubuntu-m2', 'env', '-C', '/home/fenhl/wslgit/github.com/dasgefolge/sil', '/home/fenhl/.cargo/bin/cargo', 'check'], check=True)
    subprocess.run(['wsl', 'nix', 'build', '--no-link'], check=True)
