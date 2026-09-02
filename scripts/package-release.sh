#!/usr/bin/env bash
set -euo pipefail

root_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$root_dir/Cargo.toml" | head -1)
target_dir="$root_dir/target/release-package"
package_dir="$target_dir/ga4-omarchy-$version"

cargo build --release --locked --manifest-path "$root_dir/Cargo.toml"
rm -rf -- "$target_dir"
install -Dm755 "$root_dir/target/release/ga4" "$package_dir/bin/ga4"
install -Dm644 "$root_dir/assets/org.omarchy.ga4.desktop" "$package_dir/share/applications/org.omarchy.ga4.desktop"
install -Dm644 "$root_dir/assets/org.omarchy.ga4.svg" "$package_dir/share/icons/hicolor/scalable/apps/org.omarchy.ga4.svg"
install -Dm644 "$root_dir/man/ga4.1" "$package_dir/share/man/man1/ga4.1"
install -Dm644 "$root_dir/completions/ga4.bash" "$package_dir/share/bash-completion/completions/ga4"
install -Dm644 "$root_dir/completions/_ga4" "$package_dir/share/zsh/site-functions/_ga4"
install -Dm644 "$root_dir/completions/ga4.fish" "$package_dir/share/fish/vendor_completions.d/ga4.fish"
install -Dm644 "$root_dir/LICENSE" "$package_dir/share/licenses/ga4-omarchy/LICENSE"
install -Dm644 "$root_dir/README.md" "$package_dir/share/doc/ga4-omarchy/README.md"
tar -C "$target_dir" -czf "$target_dir/ga4-omarchy-$version-x86_64-linux-gnu.tar.gz" "ga4-omarchy-$version"
sha256sum "$target_dir/ga4-omarchy-$version-x86_64-linux-gnu.tar.gz" > "$target_dir/ga4-omarchy-$version-x86_64-linux-gnu.tar.gz.sha256"
echo "Created release assets in $target_dir"
