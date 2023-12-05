# Local Development

Windows and (ubuntu-ish) linux are supported. It might work on other platforms and it might not.
To build from source on linux, you'll need some native dependencies. Using apt:

```sh
sudo apt install libsqlite3-dev cmake libfontconfig1-dev
```

For development, you'll also want
[just](https://github.com/casey/just),
[bacon](https://dystroy.org/bacon), and
[diesel_cli](https://crates.io/crates/diesel_cli):

```sh
cargo install just
cargo install --locked bacon
```

On linux, make sure diesel_cli includes only sqlite:

```sh
cargo install diesel_cli --no-default-features --features sqlite
```

On windows, it's easier to have diesel_cli use bundled sqlite than to install your own copy:

```sh
cargo install diesel_cli --no-default-features --features "sqlite-bundled"
```

To use diesel_cli to add migrations, on linux you'll need to copy the .env example:

```sh
cp .env.example .env
```

On windows, the DATABASE_URL needs to be set manually in powershell:

```powershell
$env:DATABASE_URL = "$HOME\AppData\Local\Clef\data\db.sqlite"
```
