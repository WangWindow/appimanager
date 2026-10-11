# AppImage Manager

使用 Rust、GTK4 和 libadwaita 构建的本地 AppImage 管理器。支持导入、管理和移除 AppImage，并提供英文与简体中文界面。

## 运行

需要 GTK4、libadwaita，以及构建工具 `msgfmt` 和 `glib-compile-schemas`。

```sh
cargo run
```

## 命令行

```sh
cargo run -- --integrate ./Example.AppImage
cargo run -- --list-installed --json
cargo run -- --remove ./Example.AppImage
```
