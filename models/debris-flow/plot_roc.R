#!/usr/bin/env Rscript
# Curva ROC del modelo de detritos enriquecido (Chañaral), desde el mapa de
# probabilidad de ensemble. Lee outputs/roc_chanaral.csv (fpr,tpr,threshold).

suppressPackageStartupMessages(library(ggplot2))

here <- dirname(sub("--file=", "", grep("--file=", commandArgs(FALSE), value = TRUE)))
out <- file.path(here, "outputs")
roc <- read.csv(file.path(out, "roc_chanaral.csv"))
roc <- roc[order(roc$fpr, roc$tpr), ]

# AUC por trapecios.
auc <- sum(diff(roc$fpr) * (head(roc$tpr, -1) + tail(roc$tpr, -1)) / 2)

# Punto óptimo de Youden (maximiza TPR - FPR).
j <- roc$tpr - roc$fpr
opt <- roc[which.max(j), ]

p <- ggplot(roc, aes(fpr, tpr)) +
  geom_abline(slope = 1, intercept = 0, linetype = "dashed",
              colour = "grey70", linewidth = 0.4) +
  geom_line(colour = "#1b5e9c", linewidth = 0.9) +
  geom_area(fill = "#1b5e9c", alpha = 0.08) +
  geom_point(data = opt, aes(fpr, tpr), colour = "#c0392b", size = 2.4) +
  annotate("text", x = opt$fpr + 0.03, y = opt$tpr - 0.04,
           label = sprintf("Youden (umbral %.2f)\nTPR %.2f, FPR %.2f",
                           opt$threshold, opt$tpr, opt$fpr),
           hjust = 0, size = 3.1, colour = "#c0392b") +
  annotate("text", x = 0.62, y = 0.18,
           label = sprintf("AUC = %.3f", auc), size = 4.6, fontface = "bold") +
  coord_equal(xlim = c(0, 1), ylim = c(0, 1), expand = FALSE) +
  labs(x = "Tasa de falsos positivos (1 − especificidad)",
       y = "Tasa de verdaderos positivos (sensibilidad)",
       title = "ROC — flujos de detritos, Chañaral",
       subtitle = "Modelo enriquecido sobre swarm-core, ensemble de 50 corridas") +
  theme_minimal(base_size = 12) +
  theme(panel.grid.minor = element_blank(),
        plot.title = element_text(face = "bold"),
        plot.subtitle = element_text(colour = "grey40", size = 9.5))

ggsave(file.path(out, "roc_chanaral.png"), p, width = 5.2, height = 5.2,
       dpi = 200, bg = "white")
cat(sprintf("AUC = %.4f -> %s\n", auc, file.path(out, "roc_chanaral.png")))
