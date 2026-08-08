check:
    @echo "🎨 フォーマットチェック..."
    cargo fmt --check
    @echo "🧪 clippy..."
    cargo clippy --all-targets -- -D warnings
    @echo "🧪 テスト..."
    cargo test
    @echo "🔍 Python tools の構文チェック..."
    python3 -m py_compile tools/update_spec.py
    python3 -m py_compile tools/make_update_package/make_update_package.py
    python3 -m py_compile tools/make_factory_image/make_factory_image.py
    @echo "✅ check 完了"

fmt:
    cargo fmt
