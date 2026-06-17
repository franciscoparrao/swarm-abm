#!/usr/bin/env Rscript
# Escalamiento del motor: rendimiento sostenido (millones de agente-pasos por
# segundo) al crecer el número de agentes móviles, un hilo. Datos de criterion
# (caminantes aleatorios). Salida: outputs/scaling.png

suppressPackageStartupMessages({ library(ggplot2); library(scales) })

here <- dirname(sub("--file=", "", grep("--file=", commandArgs(FALSE), value = TRUE)))
df <- read.csv(file.path(here, "data/scaling.csv"))
df$steps_per_s <- 1000 / df$ms_step  # pasos completos por segundo

lab <- sprintf("%.0f M a-p/s\n(%s pasos/s)", df$throughput_Mps,
               formatC(df$steps_per_s, format = "f", digits = if (max(df$ms_step) > 10) 0 else 0))

p <- ggplot(df, aes(agents, throughput_Mps)) +
  geom_line(colour = "#0072B2", linewidth = 1.1) +
  geom_point(colour = "#0072B2", size = 3) +
  geom_text(aes(label = sprintf("%.0f M/s", throughput_Mps)),
            vjust = -1.0, size = 3.2, fontface = "bold", colour = "#0072B2") +
  scale_x_log10(breaks = df$agents,
                labels = c("10 mil", "100 mil", "1 millón")) +
  scale_y_continuous(limits = c(0, 42), breaks = seq(0, 40, 10)) +
  labs(x = "Número de agentes (escala log)",
       y = "Rendimiento (millones de agente-pasos/s)",
       title = "Escalamiento del motor — caminantes aleatorios, 1 hilo",
       subtitle = "Sostiene millones de agentes en tiempo real; a 1 M baja por presión de caché") +
  theme_minimal(base_size = 12) +
  theme(plot.title = element_text(face = "bold"),
        plot.subtitle = element_text(colour = "grey40", size = 9.5),
        panel.grid.minor = element_blank())

ggsave(file.path(here, "outputs/scaling.png"), p, width = 6.4, height = 4.6,
       dpi = 200, bg = "white")
cat("-> scaling.png\n")
