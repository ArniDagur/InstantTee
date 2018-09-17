<h1 align="center">
  <img src="http://i.imgur.com/1KDSE7T.gif" alt="K-On! Tea GIF"/><br>
  Instant Tee
</h1>
<p align="center">Because noone likes waiting for their Tee.</p>

```bash
$ fcat /tmp/largefile | itee | pv -r > /dev/zero
[11.5GiB/s]
$ fcat /tmp/largefile | tee | pv -r > /dev/zero
[2.54GiB/s]
$ fcat /tmp/largefile | itee /dev/zero | pv -r > /dev/zero
[5.31GiB/s]
$ fcat /tmp/largefile | tee /dev/zero | pv -r > /dev/zero
[2.34GiB/s]
```

<h2>License</h2>
This project is licensed under the MIT license. See <a href="https://github.com/ArniDagur/InstantTee/blob/master/LICENSE">LICENSE</a> for the full license text.
