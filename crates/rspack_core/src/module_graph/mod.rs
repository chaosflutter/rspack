use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::sync::atomic::{AtomicUsize, Ordering};

use rspack_error::{internal_error, Result};
use rspack_identifier::IdentifierMap;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

mod connection;
pub use connection::{ConnectionId, ModuleGraphConnection};

use crate::{
  BoxModule, BoxModuleDependency, DependencyId, Module, ModuleGraphModule, ModuleIdentifier,
};

#[derive(Debug, Default)]
pub struct ModuleGraph {
  dependency_id_to_module_identifier: HashMap<DependencyId, ModuleIdentifier>,

  /// Module identifier to its module
  module_identifier_to_module: IdentifierMap<BoxModule>,

  /// Module identifier to its module graph module
  module_identifier_to_module_graph_module: IdentifierMap<ModuleGraphModule>,

  dependency_id_to_connection_id: HashMap<DependencyId, ConnectionId>,
  connection_id_to_dependency_id: HashMap<ConnectionId, DependencyId>,
  dependency_id_to_dependency: HashMap<DependencyId, BoxModuleDependency>,

  /// The module graph connections
  connections: HashSet<ModuleGraphConnection>,
  connection_id_to_connection: HashMap<ConnectionId, ModuleGraphConnection>,
}

impl ModuleGraph {
  /// Return an unordered iterator of modules
  pub fn modules(&self) -> &IdentifierMap<BoxModule> {
    &self.module_identifier_to_module
  }

  pub fn modules_mut(&mut self) -> &mut IdentifierMap<BoxModule> {
    &mut self.module_identifier_to_module
  }

  pub fn module_graph_modules(&self) -> &IdentifierMap<ModuleGraphModule> {
    &self.module_identifier_to_module_graph_module
  }

  pub fn add_module_graph_module(&mut self, module_graph_module: ModuleGraphModule) {
    if let Entry::Vacant(val) = self
      .module_identifier_to_module_graph_module
      .entry(module_graph_module.module_identifier)
    {
      val.insert(module_graph_module);
    }
  }

  pub fn add_module(&mut self, module: BoxModule) {
    if let Entry::Vacant(val) = self.module_identifier_to_module.entry(module.identifier()) {
      val.insert(module);
    }
  }

  pub fn add_dependency(&mut self, mut dep: BoxModuleDependency) -> DependencyId {
    static NEXT_DEPENDENCY_ID: AtomicUsize = AtomicUsize::new(0);

    if let Some(id) = dep.id() {
      return id;
    }
    let id = NEXT_DEPENDENCY_ID.fetch_add(1, Ordering::Relaxed);
    let id = DependencyId::from(id);
    dep.set_id(Some(id));
    self.dependency_id_to_dependency.insert(id, dep);

    id
  }

  pub fn dependency_by_id(&self, id: &DependencyId) -> Option<&BoxModuleDependency> {
    self.dependency_id_to_dependency.get(id)
  }

  /// Uniquely identify a module by its dependency
  pub fn module_graph_module_by_dependency_id(
    &self,
    id: &DependencyId,
  ) -> Option<&ModuleGraphModule> {
    self
      .module_identifier_by_dependency_id(id)
      .and_then(|module_identifier| {
        self
          .module_identifier_to_module_graph_module
          .get(module_identifier)
      })
  }

  pub fn module_identifier_by_dependency_id(&self, id: &DependencyId) -> Option<&ModuleIdentifier> {
    self.dependency_id_to_module_identifier.get(id)
  }

  /// Add a connection between two module graph modules, if a connection exists, then it will be reused.
  pub fn set_resolved_module(
    &mut self,
    original_module_identifier: Option<ModuleIdentifier>,
    dependency_id: DependencyId,
    module_identifier: ModuleIdentifier,
  ) -> Result<()> {
    self
      .dependency_id_to_module_identifier
      .insert(dependency_id, module_identifier);
    let new_connection =
      ModuleGraphConnection::new(original_module_identifier, dependency_id, module_identifier);

    let connection_id = if let Some(connection) = self.connections.get(&new_connection) {
      connection.id
    } else {
      let id = new_connection.id;
      self.connections.insert(new_connection.clone());
      self.connection_id_to_connection.insert(id, new_connection);
      id
    };

    self
      .dependency_id_to_connection_id
      .insert(dependency_id, connection_id);

    self
      .connection_id_to_dependency_id
      .insert(connection_id, dependency_id);

    {
      let mgm = self
        .module_graph_module_by_identifier_mut(&module_identifier)
        .ok_or_else(|| {
          internal_error!(
            "Failed to set resolved module: Module linked to module identifier {module_identifier} cannot be found"
          )
        })?;

      mgm.add_incoming_connection(connection_id);
    }

    if let Some(identifier) = original_module_identifier && let Some(original_mgm) = self.
    module_graph_module_by_identifier_mut(&identifier) {
        original_mgm.add_outgoing_connection(connection_id);
    };

    Ok(())
  }

  /// Uniquely identify a module by its identifier and return the aliased reference
  #[inline]
  pub fn module_by_identifier(&self, identifier: &ModuleIdentifier) -> Option<&BoxModule> {
    self.module_identifier_to_module.get(identifier)
  }

  /// Aggregate function which combine `get_normal_module_by_identifier`, `as_normal_module`, `get_resource_resolved_data`
  #[inline]
  pub fn normal_module_source_path_by_identifier(
    &self,
    identifier: &ModuleIdentifier,
  ) -> Option<Cow<str>> {
    self
      .module_identifier_to_module
      .get(identifier)
      .and_then(|module| module.as_normal_module())
      .map(|module| {
        module
          .resource_resolved_data()
          .resource_path
          .to_string_lossy()
      })
  }

  /// Uniquely identify a module by its identifier and return the exclusive reference
  #[inline]
  pub fn module_by_identifier_mut(
    &mut self,
    identifier: &ModuleIdentifier,
  ) -> Option<&mut BoxModule> {
    self.module_identifier_to_module.get_mut(identifier)
  }

  /// Uniquely identify a module graph module by its module's identifier and return the aliased reference
  #[inline]
  pub fn module_graph_module_by_identifier(
    &self,
    identifier: &ModuleIdentifier,
  ) -> Option<&ModuleGraphModule> {
    self
      .module_identifier_to_module_graph_module
      .get(identifier)
  }

  /// Uniquely identify a module graph module by its module's identifier and return the exclusive reference
  #[inline]
  pub fn module_graph_module_by_identifier_mut(
    &mut self,
    identifier: &ModuleIdentifier,
  ) -> Option<&mut ModuleGraphModule> {
    self
      .module_identifier_to_module_graph_module
      .get_mut(identifier)
  }

  /// Uniquely identify a connection by a given dependency
  pub fn connection_by_dependency(
    &self,
    dependency_id: &DependencyId,
  ) -> Option<&ModuleGraphConnection> {
    self
      .dependency_id_to_connection_id
      .get(dependency_id)
      .and_then(|id| self.connection_id_to_connection.get(id))
  }

  /// Get a list of all dependencies of a module by the module itself, if the module is not found, then None is returned
  pub fn dependencies_by_module(&self, module: &dyn Module) -> Option<&[DependencyId]> {
    self.dependencies_by_module_identifier(&module.identifier())
  }

  /// Get a list of all dependencies of a module by the module identifier, if the module is not found, then None is returned
  pub fn dependencies_by_module_identifier(
    &self,
    module_identifier: &ModuleIdentifier,
  ) -> Option<&[DependencyId]> {
    self
      .module_graph_module_by_identifier(module_identifier)
      .map(|mgm| mgm.dependencies.as_slice())
  }

  pub fn dependency_by_connection(
    &self,
    connection: &ModuleGraphConnection,
  ) -> Option<&BoxModuleDependency> {
    self.dependency_by_connection_id(&connection.id)
  }

  pub fn dependency_by_connection_id(
    &self,
    connection_id: &ConnectionId,
  ) -> Option<&BoxModuleDependency> {
    self
      .connection_id_to_dependency_id
      .get(connection_id)
      .and_then(|id| self.dependency_id_to_dependency.get(id))
  }

  pub fn connection_by_connection_id(
    &self,
    connection_id: &ConnectionId,
  ) -> Option<&ModuleGraphConnection> {
    self.connection_id_to_connection.get(connection_id)
  }

  pub fn remove_connection_by_dependency(
    &mut self,
    id: &DependencyId,
  ) -> Option<ModuleGraphConnection> {
    let mut removed = None;

    if let Some(conn) = self.dependency_id_to_connection_id.remove(id) {
      self.connection_id_to_dependency_id.remove(&conn);

      if let Some(conn) = self.connection_id_to_connection.remove(&conn) {
        self.connections.remove(&conn);

        if let Some(mgm) = conn
          .original_module_identifier
          .as_ref()
          .and_then(|ident| self.module_graph_module_by_identifier_mut(ident))
        {
          mgm.outgoing_connections.remove(&conn.id);
        };

        if let Some(mgm) = self.module_graph_module_by_identifier_mut(&conn.module_identifier) {
          mgm.incoming_connections.remove(&conn.id);
        }

        removed = Some(conn);
      }
    }
    self.dependency_id_to_module_identifier.remove(id);
    self.dependency_id_to_dependency.remove(id);

    removed
  }

  pub fn get_pre_order_index(&self, module_identifier: &ModuleIdentifier) -> Option<usize> {
    self
      .module_graph_module_by_identifier(module_identifier)
      .and_then(|mgm| mgm.pre_order_index)
  }

  pub fn get_issuer(&self, module: &BoxModule) -> Option<&BoxModule> {
    self
      .module_graph_module_by_identifier(&module.identifier())
      .and_then(|mgm| mgm.get_issuer().get_module(self))
  }

  pub fn get_outgoing_connections(&self, module: &BoxModule) -> HashSet<&ModuleGraphConnection> {
    self
      .module_graph_module_by_identifier(&module.identifier())
      .map(|mgm| {
        mgm
          .outgoing_connections
          .iter()
          .filter_map(|id| self.connection_by_connection_id(id))
          .collect()
      })
      .unwrap_or_default()
  }

  /// Remove a connection and return connection origin module identifier and dependency
  fn revoke_connection(&mut self, cid: ConnectionId) -> Option<DependencyId> {
    let connection = match self.connection_id_to_connection.remove(&cid) {
      Some(c) => c,
      None => return None,
    };
    self.connections.remove(&connection);

    let ModuleGraphConnection {
      original_module_identifier,
      module_identifier,
      dependency_id,
      ..
    } = connection;

    // remove dependency
    self.dependency_id_to_connection_id.remove(&dependency_id);
    self
      .dependency_id_to_module_identifier
      .remove(&dependency_id);

    // remove outgoing from original module graph module
    if let Some(original_module_identifier) = &original_module_identifier {
      if let Some(mgm) = self
        .module_identifier_to_module_graph_module
        .get_mut(original_module_identifier)
      {
        mgm.outgoing_connections.remove(&cid);
        // Because of mgm.dependencies is set when original module build success
        // it does not need to remove dependency in mgm.dependencies.
      }
    }
    // remove incoming from module graph module
    if let Some(mgm) = self
      .module_identifier_to_module_graph_module
      .get_mut(&module_identifier)
    {
      mgm.incoming_connections.remove(&cid);
    }

    Some(dependency_id)
  }

  /// Remove module from module graph and return parent module identifier and dependency pair
  pub fn revoke_module(&mut self, module_identifier: &ModuleIdentifier) -> Vec<DependencyId> {
    self.module_identifier_to_module.remove(module_identifier);
    let mgm = self
      .module_identifier_to_module_graph_module
      .remove(module_identifier);

    if let Some(mgm) = mgm {
      for cid in mgm.outgoing_connections {
        self.revoke_connection(cid);
      }

      mgm
        .incoming_connections
        .iter()
        .filter_map(|cid| self.revoke_connection(*cid))
        .collect()
    } else {
      vec![]
    }
  }

  pub fn set_module_hash(&mut self, module_identifier: &ModuleIdentifier, hash: u64) {
    if let Some(mgm) = self.module_graph_module_by_identifier_mut(module_identifier) {
      mgm.hash = Some(hash);
    }
  }

  #[inline]
  pub fn get_module_hash(&self, module_identifier: &ModuleIdentifier) -> Option<u64> {
    self
      .module_graph_module_by_identifier(module_identifier)
      .and_then(|mgm| mgm.hash)
  }
}

#[cfg(test)]
mod test {
  use std::borrow::Cow;

  use rspack_error::{Result, TWithDiagnosticArray};
  use rspack_identifier::Identifiable;
  use rspack_sources::Source;

  use crate::{
    BuildContext, BuildResult, CodeGeneratable, CodeGenerationResult, Compilation, Context,
    Dependency, DependencyId, Module, ModuleDependency, ModuleGraph, ModuleGraphModule,
    ModuleIdentifier, ModuleType, SourceType,
  };

  // Define a detailed node type for `ModuleGraphModule`s
  #[derive(Debug, PartialEq, Eq, Hash)]
  struct Node(&'static str);

  macro_rules! impl_noop_trait_module_type {
    ($ident:ident) => {
      impl Identifiable for $ident {
        fn identifier(&self) -> ModuleIdentifier {
          (stringify!($ident).to_owned() + "__" + self.0).into()
        }
      }

      #[::async_trait::async_trait]
      impl Module for $ident {
        fn module_type(&self) -> &ModuleType {
          unreachable!()
        }

        fn source_types(&self) -> &[SourceType] {
          unreachable!()
        }

        fn original_source(&self) -> Option<&dyn Source> {
          unreachable!()
        }

        fn size(&self, _source_type: &SourceType) -> f64 {
          unreachable!()
        }

        fn readable_identifier(&self, _context: &Context) -> Cow<str> {
          unreachable!()
        }

        async fn build(
          &mut self,
          _build_context: BuildContext<'_>,
        ) -> Result<TWithDiagnosticArray<BuildResult>> {
          unreachable!()
        }

        fn code_generation(&self, _compilation: &Compilation) -> Result<CodeGenerationResult> {
          unreachable!()
        }
      }
    };
  }

  impl_noop_trait_module_type!(Node);

  // Define a detailed edge type for `ModuleGraphConnection`s, tuple contains the parent module identifier and the child module specifier
  #[derive(Debug, PartialEq, Eq, Hash, Clone)]
  struct Edge(Option<ModuleIdentifier>, String);

  macro_rules! impl_noop_trait_dep_type {
    ($ident:ident) => {
      impl Dependency for $ident {
        fn parent_module_identifier(&self) -> Option<&ModuleIdentifier> {
          self.0.as_ref()
        }
      }

      impl ModuleDependency for $ident {
        fn request(&self) -> &str {
          &*self.1
        }

        fn user_request(&self) -> &str {
          &*self.1
        }

        fn span(&self) -> Option<&crate::ErrorSpan> {
          unreachable!()
        }
      }

      impl CodeGeneratable for $ident {
        fn generate(
          &self,
          _code_generatable_context: &mut crate::CodeGeneratableContext,
        ) -> Result<crate::CodeGeneratableResult> {
          unreachable!()
        }
      }
    };
  }

  impl_noop_trait_dep_type!(Edge);

  fn add_module_to_graph(mg: &mut ModuleGraph, m: Box<dyn Module>) {
    let mgm = ModuleGraphModule::new(m.identifier(), ModuleType::Js, true);
    mg.add_module_graph_module(mgm);
    mg.add_module(m);
  }

  fn link_modules_with_dependency(
    mg: &mut ModuleGraph,
    from: Option<&ModuleIdentifier>,
    to: &ModuleIdentifier,
    dep: Box<dyn ModuleDependency>,
  ) -> DependencyId {
    let dependency_id = mg.add_dependency(dep);
    mg.dependency_id_to_module_identifier
      .insert(dependency_id, *to);
    if let Some(p_id) = from && let Some(mgm) = mg.module_graph_module_by_identifier_mut(p_id) {
      mgm.dependencies.push(dependency_id);
    }
    mg.set_resolved_module(from.copied(), dependency_id, *to)
      .expect("failed to set resolved module");

    assert_eq!(
      mg.dependency_id_to_module_identifier
        .get(&dependency_id)
        .copied(),
      Some(*to)
    );
    dependency_id
  }

  fn mgm<'m>(mg: &'m ModuleGraph, m_id: &ModuleIdentifier) -> &'m ModuleGraphModule {
    mg.module_graph_module_by_identifier(m_id)
      .expect("not found")
  }

  macro_rules! node {
    ($s:literal) => {
      Node($s)
    };
  }

  macro_rules! edge {
    ($from:literal, $to:expr) => {
      Edge(Some($from.into()), $to.into())
    };
    ($from:expr, $to:expr) => {
      Edge($from, $to.into())
    };
  }

  #[test]
  fn test_module_graph() {
    let mut mg = ModuleGraph::default();
    let a = node!("a");
    let b = node!("b");
    let a_id = a.identifier();
    let b_id = b.identifier();
    let a_to_b = edge!(Some(a_id), b_id.as_str());
    add_module_to_graph(&mut mg, box a);
    add_module_to_graph(&mut mg, box b);
    let a_to_b_id = link_modules_with_dependency(&mut mg, Some(&a_id), &b_id, box (a_to_b));

    let mgm_a = mgm(&mg, &a_id);
    let mgm_b = mgm(&mg, &b_id);
    let conn_a = mgm_a.outgoing_connections.iter().collect::<Vec<_>>();
    let conn_b = mgm_b.incoming_connections.iter().collect::<Vec<_>>();
    assert_eq!(conn_a[0], conn_b[0]);

    let c = node!("c");
    let c_id = c.identifier();
    let b_to_c = edge!(Some(b_id), c_id.as_str());
    add_module_to_graph(&mut mg, box c);
    let b_to_c_id = link_modules_with_dependency(&mut mg, Some(&b_id), &c_id, box (b_to_c));

    let mgm_b = mgm(&mg, &b_id);
    let mgm_c = mgm(&mg, &c_id);
    let conn_b = mgm_b.outgoing_connections.iter().collect::<Vec<_>>();
    let conn_c = mgm_c.incoming_connections.iter().collect::<Vec<_>>();
    assert_eq!(conn_c[0], conn_b[0]);

    mg.remove_connection_by_dependency(&a_to_b_id);

    let mgm_a = mgm(&mg, &a_id);
    let mgm_b = mgm(&mg, &b_id);
    assert!(mgm_a.outgoing_connections.is_empty());
    assert!(mgm_b.incoming_connections.is_empty());

    mg.remove_connection_by_dependency(&b_to_c_id);

    let mgm_b = mgm(&mg, &b_id);
    let mgm_c = mgm(&mg, &c_id);
    assert!(mgm_b.outgoing_connections.is_empty());
    assert!(mgm_c.incoming_connections.is_empty());
  }
}