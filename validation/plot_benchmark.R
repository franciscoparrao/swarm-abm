#!/usr/bin/env Rscript
# Benchmark cross-engine swarm-core (Rust) vs Mesa (Python): tiempo por paso
# vs número de agentes (SIR espacial idéntico). Salida: outputs/benchmark_mesa.png

suppressPackageStartupMessages({ library(ggplot2); library(scales) })

here <- dirname(sub("--file=", "", grep("--file=", commandArgs(FALSE), value = TRUE)))
df <- read.csv(file.path(here, "data/bench_summary.csv"))

long <- data.frame(
  agents = rep(df$agents, 2),
  engine = factor(rep(c("swarm-core (Rust)", "Mesa (Python)"), each = nrow(df)),
                  levels = c("Mesa (Python)", "swarm-core (Rust)")),
  ms = c(df$rust, df$mesa)
)
# Etiquetas de speedup, posicionadas entre las dos curvas.
lab <- data.frame(agents = df$agents,
                  ms = sqrt(df$mesa * df$rust),
                  label = sprintf("%.0f×", df$speedup))

pal <- c("Mesa (Python)" = "#888888", "swarm-core (Rust)" = "#0072B2")

p <- ggplot(long, aes(agents, ms, colour = engine)) +
  geom_line(linewidth = 1.1) +
  geom_point(size = 2.6) +
  geom_text(data = lab, aes(agents, ms, label = label), inherit.aes = FALSE,
            colour = "grey25", fontface = "bold", size = 3.2, nudge_x = 0.04) +
  scale_x_log10(breaks = df$agents,
                labels = label_number(big.mark = ".", decimal.mark = ",")) +
  scale_y_log10(labels = label_number(accuracy = 0.01)) +
  scale_colour_manual(values = pal, name = NULL) +
  annotation_logticks(sides = "lb", colour = "grey75") +
  labs(x = "Número de agentes", y = "Tiempo por paso (ms, escala log)",
       title = "swarm-core vs Mesa — mismo modelo SIR espacial",
       subtitle = "Medición en proceso, mediana sobre semillas · etiqueta = speedup") +
  theme_minimal(base_size = 12) +
  theme(legend.position = "top",
        plot.title = element_text(face = "bold"),
        plot.subtitle = element_text(colour = "grey40", size = 9.5),
        panel.grid.minor = element_blank())

ggsave(file.path(here, "outputs/benchmark_mesa.png"),
       p, width = 6.4, height = 5, dpi = 200, bg = "white")
cat("-> benchmark_mesa.png\n")
