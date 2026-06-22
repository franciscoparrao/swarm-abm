# Theme y helpers publication-quality para las figuras de swarm-abm (SIMPAT).
# Estilo Elsevier doble columna. Paleta Wong (colorblind-safe). Sin ggsci.

suppressPackageStartupMessages({
  library(ggplot2)
})

# Paleta Wong (8 colores, colorblind-safe).
wong <- c(
  black   = "#000000", orange = "#E69F00", skyblue = "#56B4E9",
  green   = "#009E73", yellow = "#F0E442", blue    = "#0072B2",
  verm    = "#D55E00", purple = "#CC79A7"
)

# Asignación semántica de motores.
engine_colors <- c(
  "swarm-abm" = unname(wong["blue"]),
  "Agents.jl" = unname(wong["orange"]),
  "Mesa"      = unname(wong["verm"])
)

# Fuente sans robusta en Linux (Liberation Sans viene siempre; cae a "sans").
.base_family <- if ("Liberation Sans" %in% systemfonts::system_fonts()$family) {
  "Liberation Sans"
} else {
  "sans"
}

theme_paper <- function(base_size = 9) {
  theme_classic(base_size = base_size, base_family = .base_family) +
    theme(
      axis.line       = element_line(linewidth = 0.3, colour = "grey20"),
      axis.ticks      = element_line(linewidth = 0.3, colour = "grey20"),
      axis.text       = element_text(colour = "grey20"),
      axis.title      = element_text(colour = "black"),
      legend.position = "top",
      legend.title    = element_blank(),
      legend.key.size = unit(0.8, "lines"),
      legend.margin   = margin(0, 0, 0, 0),
      legend.box.spacing = unit(2, "pt"),
      panel.grid.major.y = element_line(linewidth = 0.2, colour = "grey90"),
      strip.background = element_blank(),
      strip.text      = element_text(face = "bold", hjust = 0, size = base_size),
      plot.tag        = element_text(face = "bold", size = base_size + 1),
      plot.margin     = margin(4, 6, 4, 4, "pt")
    )
}

# Guardado vectorial con fuentes embebidas. width/height en cm.
save_paper <- function(plot, path, width = 18, height = 8) {
  ggsave(path, plot = plot, width = width, height = height,
         units = "cm", device = cairo_pdf)
  message("guardado: ", path, " (", width, "x", height, " cm)")
}
