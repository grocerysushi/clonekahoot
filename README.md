# Rally Quiz

Rally Quiz is a self-hosted multiplayer trivia game. Rust handles the HTTP server, page rendering, game state, and scoring. The browser uses HTML and CSS and refreshes the game view automatically.

## Run locally

Install a current stable Rust toolchain, then run:

```sh
cargo run --locked
```

Open [http://127.0.0.1:3000](http://127.0.0.1:3000). The home page includes a sample quiz. Edit it or paste in your own, then choose **Create room**. Each line takes a prompt, four choices, and the correct choice number from 1 to 4.

```text
Which ocean is the largest? | Atlantic | Indian | Pacific | Arctic | 3
```

Share the six-character room code and the server address with your players. Start the game after everyone joins. Players get one answer per question and up to 20 seconds to answer. A correct answer earns 500 points plus up to 500 more based on speed. The host can reveal early and controls when the next question starts. Standings appear after each question and at the end.

To play from other devices on your local network, set `RALLY_BIND` to `0.0.0.0:3000` before starting the server and tell players to open the host computer's LAN address on port 3000. To host and join from the same device, open a private window for the player.

Rooms support up to 100 players and expire after eight hours. Quiz and game state are stored in memory and reset when the server restarts. The app is designed for small, self-hosted sessions.

## License

MIT. See [LICENSE](LICENSE).
