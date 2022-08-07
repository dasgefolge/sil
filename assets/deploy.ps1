function ThrowOnNativeFailure {
    if (-not $?)
    {
        throw 'Native Failure'
    }
}

git push
ThrowOnNativeFailure

ssh gefolge.org env -C /opt/git/github.com/dasgefolge/sil/master git pull
ThrowOnNativeFailure

ssh gefolge.org env -C /opt/git/github.com/dasgefolge/sil/master cargo build --release
ThrowOnNativeFailure

ssh gefolge.org env -C /opt/git/github.com/dasgefolge/gefolge-websocket/master git pull
ThrowOnNativeFailure

ssh gefolge.org env -C /opt/git/github.com/dasgefolge/gefolge-websocket/master cargo build --release --features=ctrlflow
ThrowOnNativeFailure

ssh gefolge.org sudo systemctl restart gefolge-websocket
ThrowOnNativeFailure
