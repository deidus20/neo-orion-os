"""orion_motion — de la palabra de la IA al movimiento seguro del robot.

    IA (habla)  →  Translator (entiende)  →  Guardian (protege)  →  Driver (mueve)

Uso rápido:
    from orion_motion import Orion
    o = Orion("profiles/brazo-6dof.json")
    for line in o.say("mueve el hombro a 45 grados y luego onda en el codo amplitud 20"):
        print(line)
"""
from .robot import Robot, Joint
from .wave import Wave
from .translator import Translator, Command
from .guardian import Guardian, Verdict
from .memory import RobotMemory


class SimDriver:
    """Driver de simulación. En el robot real se reemplaza por uno de
    servos (Feetech/Dynamixel/PCA9685) o por un publicador ROS 2."""
    def __init__(self):
        self.sent = []

    def set_joint(self, joint, deg, vel):
        self.sent.append(("SET", joint, round(deg, 2), vel))

    def play(self, joint, samples):
        self.sent.append(("TRAJ", joint, len(samples)))

    def drive(self, mm):
        self.sent.append(("DRIVE", round(mm, 1)))

    def stop(self):
        self.sent.append(("STOP",))


class Orion:
    def __init__(self, profile: str, driver=None, distance_sensor=None, memory_root=None):
        self.robot = Robot.load(profile)
        self.mem = RobotMemory(self.robot.nombre, memory_root)
        self.translator = Translator(self.robot, self.mem.macros())
        self.guardian = Guardian(self.robot, distance_sensor)
        self.driver = driver or SimDriver()

    def say(self, text: str) -> list[str]:
        """La IA 'habla'; esto devuelve lo que pasó, línea a línea."""
        self.translator.macros = self.mem.macros()
        cmds = self.translator.parse(text)
        if not cmds:
            return [f"? no entendí ninguna orden en: «{text}»"]
        out = []
        for cmd in cmds:
            verdict = self.guardian.check(cmd)
            out.append(str(verdict))
            self.mem.log({"frase": cmd.source, "orden": str(cmd), "ok": verdict.ok, "notas": verdict.notes})
            if verdict.ok:
                self._execute(cmd)
        return out

    def _execute(self, c: Command):
        r = self.robot
        if c.kind == "SET":
            r.pose[c.joint] = c.value
            self.driver.set_joint(c.joint, c.value, c.params.get("vel"))
        elif c.kind == "HOME":
            for j in r.joints:
                r.pose[j.id] = j.home
                self.driver.set_joint(j.id, j.home, j.vel_max * 0.3)
        elif c.kind == "WAVE":
            p = c.params
            w = Wave(c.joint, p["amp"], p["freq"], r.pose[c.joint], 0, p["dur"], p["mult"])
            self.driver.play(c.joint, w.samples())
        elif c.kind == "SAVE":
            self.mem.save_macro(c.params["name"], dict(r.pose))
        elif c.kind == "MACRO":
            pose = self.mem.macros()[c.params["name"]]["pose"]
            for jid, deg in pose.items():
                self.say(f"{jid} a {deg} grados")
        elif c.kind in ("DRIVE", "TURN"):
            self.driver.drive(c.value)
        elif c.kind == "STOP":
            self.driver.stop()
