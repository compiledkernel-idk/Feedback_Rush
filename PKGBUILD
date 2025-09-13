# Maintainer: compiledkernel-idk <compiledkernel-idk@users.noreply.github.com>
pkgname=feedback-rush
pkgver=0.1.0
pkgrel=1
pkgdesc="Feedback Rush — a reflex arena about outmaneuvering your past selves."
arch=('x86_64' 'aarch64')
url="https://github.com/compiledkernel-idk/feedback-rush"
license=('MIT')
depends=('glibc' 'libx11' 'libgl' 'alsa-lib')
makedepends=('rust' 'cargo' 'git')
source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')

build() {
  cd "$srcdir/$pkgname-$pkgver"
  # Try locked build if Cargo.lock exists, otherwise fall back.
  if [[ -f Cargo.lock ]]; then
    cargo build --release --locked
  else
    cargo build --release
  fi
}

check() {
  # No tests available
  :
}

package() {
  cd "$srcdir/$pkgname-$pkgver"
  install -Dm0755 "target/release/feedback-rush" "$pkgdir/usr/bin/feedback-rush"
  install -Dm0644 "LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
  install -Dm0644 "README.md" "$pkgdir/usr/share/doc/$pkgname/README.md"
}

