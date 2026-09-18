name := 'clipit'
appid := 'dev.clipit.Clipit'
rootdir := ''
prefix := '/usr'

# Falls back to anaconda's libxkbcommon; remove once libxkbcommon-dev is
# installed system-wide (sudo apt install libxkbcommon-dev).
export PKG_CONFIG_PATH := env_var_or_default('PKG_CONFIG_PATH', env('HOME') / 'anaconda3' / 'lib' / 'pkgconfig')

base-dir := absolute_path(clean(rootdir / prefix))
cargo-target-dir := env('CARGO_TARGET_DIR', 'target')
bin-dst := base-dir / 'bin' / name
desktop-dst := base-dir / 'share' / 'applications' / appid + '.desktop'
user-bin := env('HOME') / '.local' / 'bin'
user-desktop := env('HOME') / '.local' / 'share' / 'applications'

default: build-release

build-debug *args:
    cargo build {{args}}

build-release *args: (build-debug '--release' args)

check *args:
    cargo clippy {{args}} -- -W clippy::pedantic

test *args:
    cargo test {{args}}

run *args:
    env RUST_BACKTRACE=full cargo run --release {{args}}

install:
    install -Dm0755 {{ cargo-target-dir / 'release' / name }} {{bin-dst}}
    install -Dm0644 resources/{{appid}}.desktop {{desktop-dst}}

install-user:
    install -Dm0755 {{ cargo-target-dir / 'release' / name }} {{user-bin / name}}
    install -Dm0644 resources/{{appid}}.desktop {{user-desktop / appid + '.desktop'}}

uninstall:
    rm {{bin-dst}} {{desktop-dst}}

uninstall-user:
    rm {{user-bin / name}} {{user-desktop / appid + '.desktop'}}
