#!/bin/sh
# cargo's runner on macOS (.cargo/config.toml): `cargo run` starts OctoBuddy
# as OctoBuddy.app, a bundle next to the binary (its name and icon in the
# Dock, as Rinx does); any other binary cargo runs (a test) runs as it is.
bin="$1"
shift
case "$(basename "$bin")" in
  octosense-octobuddy) ;;
  *) exec "$bin" "$@" ;;
esac
here="$(cd "$(dirname "$0")" && pwd)"
contents="$(dirname "$bin")/OctoBuddy.app/Contents"
mkdir -p "$contents/MacOS" "$contents/Resources"
cp "$bin" "$contents/MacOS/OctoBuddy"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$here/../Cargo.toml" | head -1)
sed "s/<string>0.1.0<\/string>/<string>$version<\/string>/g" "$here/macos/Info.plist" > "$contents/Info.plist"
cp "$here/macos/OctoBuddy.icns" "$contents/Resources/OctoBuddy.icns"
exec "$contents/MacOS/OctoBuddy" "$@"
