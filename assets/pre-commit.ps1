#!/usr/bin/env pwsh

cargo check
if (-not $?)
{
    throw 'Native Failure'
}

# copy the tree to the WSL file system to improve compile times
wsl rsync --delete -av /mnt/c/Users/fenhl/git/github.com/dasgefolge/sil/stage/ /home/fenhl/wslgit/github.com/dasgefolge/sil/ --exclude target
if (-not $?)
{
    throw 'Native Failure'
}

wsl env -C /home/fenhl/wslgit/github.com/dasgefolge/sil cargo check
if (-not $?)
{
    throw 'Native Failure'
}

wsl -d nixos-m2 nix build --no-link
if (-not $?)
{
    throw 'Native Failure'
}
