# Drawing effect targets

Run `cargo run --release -p dream-drawing --example visual_bench` on a machine with a GPU.
It measures a 320 by 240 canvas, one cold render and 30 subsequent renders per scene.
Times below measure CPU rendering/submission work, not GPU execution time. Completion and
pixel readback happen outside the timer. These are local Windows measurements, not CI budgets.

The before/after columns compare a cold target pool with the warmed pool. Scratch allocations
count full-surface targets owned by the drawing engine; Vello internals, bind groups and driver
allocations are excluded. Zero warm allocations do not imply the whole renderer allocates nothing.

| Backend | Scene | Cold CPU us | Warm CPU us/frame | Cold scratch targets | New warm targets | Resident bytes |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| cpu | unfiltered | 814.3 | 483.4 | 0 | 0 | 0 |
| cpu | filtered | 5126.1 | 4956.3 | 2 | 0 | 614400 |
| cpu | acrylic | 9088.5 | 9003.0 | 0 | 0 | 0 |
| gpu | unfiltered | 412.3 | 110.2 | 0 | 0 | 0 |
| gpu | filtered | 1071.6 | 413.4 | 4 | 0 | 1228800 |
| gpu | acrylic | 2104.7 | 1026.8 | 0 | 0 | 0 |

All measured render loops performed zero GPU pixel reads. The acrylic baseline here uses three
blurred/tinted panels; the following materials stage adds noise and nested panel coverage.
