# arora-aldebaran

The device runner of Aldebaran robots (NAO, Pepper): `arora-aldebaran` connects to the robot
through the HAL its build selects (`robot`; feature `naoqi`, the default), runs a behavior
tree on it, and serves it to Arora's bridges, Semio Studio with the `studio` feature.

The library holds what does not depend on the HAL: the trees
([`trees/ready.groot.xml`](trees/ready.groot.xml), the default behavior, and
[`trees/touch.groot.xml`](trees/touch.groot.xml)), the keys the default behavior seeds
(`ready_tree_initial_state`) and the gesture leaves (`gestures`).

The [repository's README](../../README.md) documents it: how to
[try it on a NAO](../../README.md#try-it-on-a-nao),
[deploy it on the robot](../../README.md#deploy-it-on-the-robot), connect it to
[Semio Studio](../../README.md#semio-studio), its [behaviors](../../README.md#behaviors) and
[choosing a HAL](../../README.md#choosing-a-hal).
