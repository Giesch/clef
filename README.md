Clef is a native local music player, similar to classic iTunes or MediaMonkey. It's built on top of [Iced](https://github.com/iced-rs/iced) and [Symphonia](https://github.com/pdeljanov/Symphonia).

![music player screenshot](./screenshot.png)

My short-term goal is to make something that feels nicer to use than streaming services, to the point that I prefer to use it myself every day.

For now, only linux is supported. To build from source, you'll need some native dependencies. Using apt:

``` sh
sudo apt install libsqlite3-dev cmake libfontconfig1-dev
```

