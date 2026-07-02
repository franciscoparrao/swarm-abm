# Figura de validación + sensibilidad del port SIGRID (swarm-abm).
# (a) Paridad distribucional Mesa vs Rust; (b) índices de Sobol (ST) del modelo
# final. Datos embebidos (ver PARITY.md y sobol_rust_N512_final.log).
# Uso: Rscript models/sigrid/figs/make_figure.R

suppressMessages({
  library(ggplot2)
  library(patchwork)
})

# Paleta Wong (colorblind-safe)
wong <- c("#0072B2", "#D55E00", "#009E73")

theme_paper <- theme_classic(base_size = 9) +
  theme(
    axis.title = element_text(size = 9),
    legend.position = "top",
    legend.title = element_text(size = 8),
    legend.key.size = unit(0.4, "cm"),
    plot.tag = element_text(face = "bold", size = 11)
  )

# --- (a) Paridad: 12 puntos factoriales, 5 semillas, 14 dias ----------------
par <- data.frame(
  mesa = c(35.8, 95.9, 15.0, 17.2, 0.2, 0.2, 57.9, 100.0, 26.8, 30.3, 0.5, 0.7),
  rust = c(38.0, 79.5, 6.3, 18.8, 12.7, 14.8, 50.7, 100.0, 23.3, 25.1, 9.5, 18.6),
  dogs = factor(c(0, 0, 1, 1, 2, 2, 0, 0, 1, 1, 2, 2))
)

pa <- ggplot(par, aes(mesa, rust, color = dogs)) +
  geom_abline(slope = 1, intercept = 0, linetype = "dashed",
              color = "grey55", linewidth = 0.3) +
  geom_point(size = 2.2, alpha = 0.9) +
  scale_color_manual(values = wong, name = "Guardian dogs") +
  scale_x_continuous(limits = c(0, 100), expand = expansion(mult = 0.02)) +
  scale_y_continuous(limits = c(0, 100), expand = expansion(mult = 0.02)) +
  coord_equal() +
  annotate("text", x = 4, y = 96, hjust = 0, size = 2.9,
           label = "r = 0.97\nRMSE = 10.1 pp") +
  labs(x = "Mesa loss rate (%)", y = "swarm-abm loss rate (%)") +
  theme_paper

# --- (b) Sobol ST del modelo final (N=512, 30 dias) -------------------------
sob <- data.frame(
  param = c("Guardian dogs", "Chilla density", "Hare density",
            "Sheep density", "Fox effectiveness", "Lamb proportion"),
  ST = c(0.994, 0.368, 0.360, 0.347, 0.342, 0.291)
)
sob$param <- factor(sob$param, levels = sob$param[order(sob$ST)])
sob$hl <- sob$param == "Guardian dogs"

pb <- ggplot(sob, aes(ST, param, fill = hl)) +
  geom_col(width = 0.7) +
  scale_fill_manual(values = c("grey70", wong[2]), guide = "none") +
  scale_x_continuous(limits = c(0, 1.05), expand = expansion(mult = c(0, 0.02))) +
  labs(x = "Total-effect Sobol index (ST)", y = NULL) +
  theme_paper

fig <- (pa | pb) +
  plot_annotation(tag_levels = "a", tag_suffix = ")") +
  plot_layout(widths = c(1, 1.1))

out <- "models/sigrid/figs/sigrid_parity_sensitivity.pdf"
ggsave(out, fig, width = 18, height = 8.5, units = "cm", device = cairo_pdf)
cat("figura guardada en", out, "\n")
