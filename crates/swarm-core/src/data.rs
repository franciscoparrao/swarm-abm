//! Recolección de series de datos por paso ([`DataCollector`]).

use std::fmt;
use std::fmt::Write as _;

/// Reporter: extrae un valor escalar del modelo.
type Reporter<M> = Box<dyn Fn(&M) -> f64>;

struct Column<M> {
    name: String,
    reporter: Reporter<M>,
    values: Vec<f64>,
}

/// Recolecta series temporales (un `f64` por paso y por reporter).
///
/// Los reporters son closures `Fn(&M) -> f64` registrados con un nombre;
/// [`collect`](Self::collect) los evalúa todos sobre el estado actual.
pub struct DataCollector<M> {
    columns: Vec<Column<M>>,
    steps: Vec<u64>,
}

impl<M> Default for DataCollector<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M> DataCollector<M> {
    /// Crea un recolector sin reporters.
    #[must_use]
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            steps: Vec::new(),
        }
    }

    /// Registra un reporter con nombre. Si el nombre ya existe, lo reemplaza
    /// (conservando los valores ya recolectados bajo ese nombre).
    pub fn add_reporter(
        &mut self,
        name: impl Into<String>,
        reporter: impl Fn(&M) -> f64 + 'static,
    ) {
        let name = name.into();
        if let Some(col) = self.columns.iter_mut().find(|c| c.name == name) {
            col.reporter = Box::new(reporter);
        } else {
            self.columns.push(Column {
                name,
                reporter: Box::new(reporter),
                values: Vec::new(),
            });
        }
    }

    /// Evalúa todos los reporters sobre `model` y anexa una fila.
    pub fn collect(&mut self, step: u64, model: &M) {
        self.steps.push(step);
        for col in &mut self.columns {
            col.values.push((col.reporter)(model));
        }
    }

    /// Serie completa de un reporter, o `None` si el nombre no existe.
    #[must_use]
    pub fn series(&self, name: &str) -> Option<&[f64]> {
        self.columns
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.values.as_slice())
    }

    /// Pasos en que se recolectó.
    #[must_use]
    pub fn steps(&self) -> &[u64] {
        &self.steps
    }

    /// Nombres de los reporters registrados, en orden de registro.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.columns.iter().map(|c| c.name.as_str()).collect()
    }

    /// Número de filas recolectadas.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// `true` si aún no se recolecta ninguna fila.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Serializa todas las series como CSV (`step,<reporters...>`).
    #[must_use]
    pub fn to_csv(&self) -> String {
        let mut out = String::from("step");
        for col in &self.columns {
            out.push(',');
            out.push_str(&col.name);
        }
        out.push('\n');
        for (i, step) in self.steps.iter().enumerate() {
            let _ = write!(out, "{step}");
            for col in &self.columns {
                let _ = write!(out, ",{}", col.values[i]);
            }
            out.push('\n');
        }
        out
    }
}

impl<M> fmt::Debug for DataCollector<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataCollector")
            .field("reporters", &self.names())
            .field("rows", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recolecta_series_por_reporter() {
        let mut dc: DataCollector<i32> = DataCollector::new();
        dc.add_reporter("doble", |m| f64::from(*m) * 2.0);
        dc.add_reporter("mitad", |m| f64::from(*m) / 2.0);

        dc.collect(0, &10);
        dc.collect(1, &20);

        assert_eq!(dc.series("doble"), Some(&[20.0, 40.0][..]));
        assert_eq!(dc.series("mitad"), Some(&[5.0, 10.0][..]));
        assert_eq!(dc.series("nada"), None);
        assert_eq!(dc.steps(), &[0, 1]);
        assert_eq!(dc.len(), 2);
    }

    #[test]
    fn csv_bien_formado() {
        let mut dc: DataCollector<i32> = DataCollector::new();
        dc.add_reporter("x", |m| f64::from(*m));
        dc.collect(0, &1);
        dc.collect(1, &2);
        assert_eq!(dc.to_csv(), "step,x\n0,1\n1,2\n");
    }

    #[test]
    fn reporter_duplicado_se_reemplaza() {
        let mut dc: DataCollector<i32> = DataCollector::new();
        dc.add_reporter("x", |_| 1.0);
        dc.add_reporter("x", |_| 2.0);
        dc.collect(0, &0);
        assert_eq!(dc.names(), vec!["x"]);
        assert_eq!(dc.series("x"), Some(&[2.0][..]));
    }
}
