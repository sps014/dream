# Choose where your program runs

Dream programs can run on your computer, in a browser, or in Node. Choose an environment based on what the program needs to do.

| Environment | Good for | Important limits |
| --- | --- | --- |
| Desktop (`native`) | Command-line tools, local files, system services | The receiving machine needs compatible system libraries |
| Browser (`web`) | Programs inside a web page | Browser permissions apply; ordinary native processes are unavailable |
| Node (`node`) | Programs launched through Node | Requires Node and the generated launcher; check each API's availability |

## Start on your computer

```sh
dreamer init hello
cd hello
dreamer run
```

For a browser or Node project, choose its environment when creating it:

```sh
dreamer init hello-web --runtime web
cd hello-web
dreamer run --target web
```

Use `--runtime node` and `--target node` for Node. A project declaring several environments requires you to choose one when running.

## Check a feature before using it

Read the availability notes in the feature guide. Running in a browser does not grant access to the user's disk or let you launch desktop programs.

See [Build and run](../reference/tooling/build-run.md) for environment selection and [Packaging](../reference/tooling/pack/index.md) for sharing the result.
