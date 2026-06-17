#!/usr/bin/env Rscript
# Mapa espacial de la predicción de flujos de detritos sobre el relieve de
# Chañaral: hillshade (SurtGIS) + probabilidad de afectación (ensemble) +
# contorno del área realmente afectada (ground truth), recortado al dominio
# urbano de evaluación. Salida: outputs/map_chanaral.png

suppressPackageStartupMessages({
  library(terra); library(ggplot2); library(tidyterra); library(ggnewscale)
})

here <- dirname(sub("--file=", "", grep("--file=", commandArgs(FALSE), value = TRUE)))
out <- file.path(here, "outputs")
res <- path.expand("~/proyectos/Agentes/resultados")

hill <- rast("/tmp/sed_work/hillshade.tif")
prob <- rast(file.path(out, "chanaral_prediction_prob.tif"))
gt   <- rast(file.path(res, "area_real_afectada.tif"))
bbox <- rast(file.path(res, "bbox_mask.tif"))

# Recorte al dominio de evaluación (bbox) con un margen para contexto.
bb <- ext(trim(ifel(bbox > 0, 1, NA)))
pad <- 0.10 * c(-(bb[2] - bb[1]), (bb[2] - bb[1]), -(bb[4] - bb[3]), (bb[4] - bb[3]))
crop_ext <- ext(bb[1] + pad[1], bb[2] + pad[2], bb[3] + pad[3], bb[4] + pad[4])

hill <- crop(hill, crop_ext); prob <- crop(prob, crop_ext)
gt <- crop(gt, crop_ext);     bbox <- crop(bbox, crop_ext)
prob <- ifel(prob >= 0.05, prob, NA)             # ruido de baja prob -> transparente
names(hill) <- "hs"; names(prob) <- "p"

# Contornos como polígonos.
gt_poly   <- as.polygons(ifel(gt == 1, 1, NA), dissolve = TRUE)
bbox_poly <- as.polygons(ifel(bbox > 0, 1, NA), dissolve = TRUE)

p <- ggplot() +
  geom_spatraster(data = hill, aes(fill = hs), show.legend = FALSE) +
  scale_fill_gradient(low = "grey25", high = "grey96", na.value = "white") +
  new_scale_fill() +
  geom_spatraster(data = prob, aes(fill = p), alpha = 0.78) +
  scale_fill_whitebox_c(palette = "bl_yl_rd", na.value = NA,
                        name = "Prob. de\nafectación", limits = c(0, 1)) +
  geom_spatvector(data = bbox_poly, fill = NA, colour = "grey20",
                  linewidth = 0.4, linetype = "dashed") +
  geom_spatvector(data = gt_poly, fill = NA, colour = "#00d0ff", linewidth = 0.6) +
  labs(title = "Predicción de flujos de detritos sobre el relieve — Chañaral",
       subtitle = "Probabilidad de ensemble (50 corridas) · AUC 0.855 · base: hillshade SurtGIS",
       caption = paste("Contorno cian: área realmente afectada (ground truth).",
                       "Borde discontinuo: dominio de evaluación.")) +
  theme_minimal(base_size = 11) +
  theme(plot.title = element_text(face = "bold", size = 12),
        plot.subtitle = element_text(colour = "grey40", size = 9),
        plot.caption = element_text(colour = "grey35", size = 8, hjust = 0),
        axis.title = element_blank(), axis.text = element_text(size = 7),
        panel.grid = element_line(colour = "grey92", linewidth = 0.2),
        plot.margin = margin(4, 6, 4, 6))

ggsave(file.path(out, "map_chanaral.png"), p, width = 7.4, height = 4.2,
       dpi = 220, bg = "white")
cat("->", file.path(out, "map_chanaral.png"), "\n")
