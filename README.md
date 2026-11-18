# Rally Quiz

Rally Quiz is a self-hosted multiplayer trivia game. Rust handles the HTTP server, page rendering, game state, and scoring. The browser uses HTML and CSS and refreshes the game view automatically.

## Run locally

Install a current stable Rust toolchain, then run:

```sh
cargo run --locked
```

Open [http://127.0.0.1:3000](http://127.0.0.1:3000) and choose **Create quiz** or **Open sample quiz**. Use **Add question** in the question sidebar to choose between four-option multiple choice and true or false. Set the correct answer and time limit for each question. Remove or reorder questions in the editor, then choose **Create room** when you are ready to host. Each quiz supports up to 30 questions.

Open **Question image** or **Image for answer** in the editor to pick a PNG, JPEG, or WebP file. Enter an optional description for screen readers and choose **Preview selected images** to see the upload before hosting. Images can be up to 512 KB each and 2 MB per quiz. Choose **Open image test quiz** on the home page to try a ready-made two-question quiz with both types of image.

Share the six-character room code and the server address with your players. Start the game after everyone joins. Players get one answer per question. The default time limit is 20 seconds. A correct answer earns 500 points plus up to 500 more based on speed. The host can reveal early and controls when the next question starts. Standings appear after each question and at the end.

To play from other devices on your local network, set `RALLY_BIND` to `0.0.0.0:3000` before starting the server and tell players to open the host computer's LAN address on port 3000. To host and join from the same device, open a private window for the player.

Rooms support up to 100 players and expire after eight hours. Quiz and game state are stored in memory and reset when the server restarts. The app is designed for small, self-hosted sessions.

## License

MIT. See [LICENSE](LICENSE).
