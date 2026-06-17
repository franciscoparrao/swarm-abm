#!/usr/bin/env Rscript
# Progresión de métricas del modelo de detritos en Chañaral a lo largo del
# ciclo de mejora dirigido por diagnóstico (4 vueltas sobre la paridad).
# Métricas fuera de muestra (8 semillas). Salida: outputs/progression_chanaral.png

suppressPackageStartupMessages(library(ggplot2))

here <- dirname(sub("--file=", "", grep("--file=", commandArgs(FALSE), value = TRUE)))
out <- file.path(here, "outputs"); dir.create(out, showWarnings = FALSE)

stages <- c("Base\n(paridad)", "+ abanico", "+ inicio\nponderado", "+ sedimento\nSurtGIS")
df <- data.frame(
  stage  = factor(rep(stages, times = 4), levels = stages),
  metric = factor(rep(c("IoU", "F1", "Precision", "Recall"), each = 4),
                  levels = c("IoU", "F1", "Precision", "Recall")),
  value  = c(0.468, 0.508, 0.543, 0.555,   # IoU
             0.638, 0.672, 0.700, 0.714,   # F1
             0.690, 0.595, 0.745, 0.825,   # Precision
             0.593, 0.789, 0.669, 0.636)   # Recall
)

# Paleta Okabe-Ito (segura para daltonismo); IoU destacado.
pal <- c(IoU = "#0072B2", F1 = "#000000", Precision = "#D55E00", Recall = "#009E73")
lw  <- c(IoU = 1.5, F1 = 1.0, Precision = 1.0, Recall = 1.0)

p <- ggplot(df, aes(stage, value, colour = metric, group = metric)) +
  geom_hline(yintercept = 0.4653, linetype = "dashed", colour = "grey65",
             linewidth = 0.4) +
  geom_line(aes(linewidth = metric)) +
  geom_point(size = 2.3) +
  geom_text(data = subset(df, metric == "IoU"), aes(label = sprintf("%.3f", value)),
            vjust = -1.1, size = 3.1, colour = pal["IoU"], fontface = "bold") +
  scale_colour_manual(values = pal, name = NULL) +
  scale_linewidth_manual(values = lw, guide = "none") +
  scale_y_continuous(limits = c(0.45, 0.86), breaks = seq(0.5, 0.85, 0.1)) +
  labs(x = NULL, y = "Métrica (fuera de muestra, n=8)",
       title = "Mejora dirigida por diagnóstico — Chañaral",
       subtitle = "Cada vuelta ataca el error dominante; IoU 0.468 → 0.555 (+19 %)",
       caption = "Línea discontinua: Config B histórico (IoU 0.465, mejor caso previo)") +
  theme_minimal(base_size = 12) +
  theme(panel.grid.minor = element_blank(),
        legend.position = "top",
        plot.title = element_text(face = "bold"),
        plot.subtitle = element_text(colour = "grey40", size = 9.5),
        axis.text.x = element_text(size = 9))

ggsave(file.path(out, "progression_chanaral.png"), p, width = 6.4, height = 5,
       dpi = 200, bg = "white")
cat("->", file.path(out, "progression_chanaral.png"), "\n")
