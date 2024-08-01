import sys
import math
import os
import matplotlib.pyplot as plt


def read_floats(filename):
    file_content = open(sys.argv[1] + "/" + filename, "r")
    floats = []
    for line in file_content.readlines():
        floats.append(float(line))
    return floats


def mean(values):
    if len(values) == 0: return 0

    m = 0
    for value in values: m += value
    return m / len(values)


def stddev(values, mean):
    if len(values) == 0: return 0

    dev = 0
    for value in values: dev += math.pow(value - mean, 2)
    return math.sqrt(dev / len(values))


if len(sys.argv) < 2:
    print("usage: " + sys.argv[0] + " <directory in [data]> <graph name>")
    exit(-1)


def export_legend(legend, filename="legend.pdf", expand=[-2,-2,2,2]):
    fig  = legend.figure
    fig.canvas.draw()
    bbox  = legend.get_window_extent()
    bbox = bbox.from_extents([
        bbox.extents[0] + expand[0],
        bbox.extents[1] + expand[1],
        bbox.extents[2] + expand[2],
        bbox.extents[3] + expand[3]]
    )
    bbox = bbox.transformed(fig.dpi_scale_trans.inverted())
    #bbox = bbox.from_extents(*(bbox.extents + np.array(expand)))
    #bbox = bbox.transformed(fig.dpi_scale_trans.inverted())
    fig.savefig(filename, dpi=300, bbox_inches=bbox)


graph_name = sys.argv[2]

directory = os.fsencode(sys.argv[1])

sequential_mean = 0
sequential_stddev = 0
items = 0

graph_data = []
for file in os.listdir(directory):
    filename = os.fsdecode(file)
    if filename == "ITEMS":
        content = open(sys.argv[1] + "/" + filename, 'r')
        items = int(content.readlines()[0])
        continue

    values = read_floats(filename)
    m = mean(values)
    dev = stddev(values, m)

    try:
        runtime, threads = filename.split('-')
        if runtime == "SPAR_RUST_MPI":
            runtime = "Our Work"
        elif runtime == "MPI":
            runtime = "Raw MPI"
        graph_data.append((runtime, int(threads) - 1, m, dev))
    except ValueError:
        if filename == "SEQUENTIAL":
            sequential_mean = m
            sequential_stddev = dev


graph_data.sort()

seq_throughput = items / sequential_mean
seq_stddev = seq_throughput - (items / (sequential_mean + sequential_stddev))
plot_data = {}
plot_data["Our Work"] = ([0], [seq_throughput], [seq_stddev])
plot_data["Raw MPI"] = ([0], [seq_throughput], [seq_stddev])

for (runtime, thread, mean, stddev) in graph_data:
    plot_data[runtime][0].append(thread)
    plot_data[runtime][1].append(items / mean)
    plot_data[runtime][2].append((items / mean) - (items / (mean + stddev)))


fig, ax1 = plt.subplots()

ax1.errorbar(
    plot_data["Raw MPI"][0],
    plot_data["Raw MPI"][1],
    plot_data["Raw MPI"][2],
    color="orange",
    label = "Raw MPI",
    fmt = 's',
    linestyle = "dashed",
    markersize = 5.0,
    elinewidth = 0.01,
    capsize = 3,
)

ax1.errorbar(
    plot_data["Our Work"][0],
    plot_data["Our Work"][1],
    plot_data["Our Work"][2],
    color="green",
    label = "Our Work",
    fmt = '^',
    linestyle = "dotted",
    markersize = 5.0,
    elinewidth = 0.01,
    capsize = 3,
)

throughput_label = "Throughput "
if graph_name == "micro-bench":
    throughput_label += "(lines/sec)"
elif graph_name == "eye-detector":
    throughput_label += "(frames/sec)"
elif graph_name == "image-processing":
    throughput_label += "(images/sec)"
else:
    throughput_label += "(chunks/sec)"

ax1.set_xlabel("Replicated Stages", fontsize=20)
ax1.set_ylabel(throughput_label, fontsize=20)

ax1.set_xticks(plot_data["Our Work"][0][1:])
# ax1.set_yticks([1, 5, 10, 15, 20, 25, 30, 35, 40, 45])
ax1.tick_params(axis='both', which='major', labelsize=16)
ax1.tick_params(axis='both', which='minor', labelsize=14)

plt.legend(loc="upper left", fontsize=16)
plt.savefig(graph_name + ".pdf", bbox_inches='tight')
