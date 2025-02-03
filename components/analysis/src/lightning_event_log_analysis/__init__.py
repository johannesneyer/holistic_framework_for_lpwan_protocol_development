import matplotlib as mpl

from .lib import (
    Action as Action,
)
from .lib import (
    ActionKind as ActionKind,
)
from .lib import (
    EventKind as EventKind,
)
from .lib import (
    Node as Node,
)
from .lib import (
    NodeId as NodeId,
)
from .lib import (
    UptimeMs as UptimeMs,
)
from .lib import (
    parse_events as parse_events,
)

mpl.rcParams["font.family"] = "sans"
mpl.rcParams["font.sans-serif"] = "Latin Modern Sans"
mpl.rcParams["font.serif"] = "Latin Modern Roman"
mpl.rcParams["font.monospace"] = "Latin Modern Mono"
mpl.rcParams["font.size"] = 12

# import matplotlib.pyplot as plt
# plt.style.use('tableau-colorblind10')
