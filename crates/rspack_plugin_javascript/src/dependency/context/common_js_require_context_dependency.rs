use rspack_core::{module_raw, AsModuleDependency, ContextDependency};
use rspack_core::{normalize_context, DependencyCategory, DependencyId, DependencyTemplate};
use rspack_core::{ContextOptions, Dependency, TemplateReplaceSource};
use rspack_core::{DependencyType, ErrorSpan, TemplateContext};

use super::create_resource_identifier_for_context_dependency;

#[derive(Debug, Clone)]
pub struct CommonJsRequireContextDependency {
  callee_start: u32,
  callee_end: u32,
  args_end: u32,
  id: DependencyId,
  options: ContextOptions,
  span: Option<ErrorSpan>,
  resource_identifier: String,
}

impl CommonJsRequireContextDependency {
  pub fn new(
    callee_start: u32,
    callee_end: u32,
    args_end: u32,
    options: ContextOptions,
    span: Option<ErrorSpan>,
  ) -> Self {
    let resource_identifier = create_resource_identifier_for_context_dependency(None, &options);
    Self {
      callee_start,
      callee_end,
      args_end,
      options,
      span,
      id: DependencyId::new(),
      resource_identifier,
    }
  }
}

impl Dependency for CommonJsRequireContextDependency {
  fn id(&self) -> &DependencyId {
    &self.id
  }

  fn category(&self) -> &DependencyCategory {
    &DependencyCategory::CommonJS
  }

  fn dependency_type(&self) -> &DependencyType {
    &DependencyType::CommonJSRequireContext
  }

  fn span(&self) -> Option<ErrorSpan> {
    self.span
  }

  fn dependency_debug_name(&self) -> &'static str {
    "CommonJsRequireContextDependency"
  }
}

impl ContextDependency for CommonJsRequireContextDependency {
  fn request(&self) -> &str {
    &self.options.request
  }

  fn options(&self) -> &ContextOptions {
    &self.options
  }

  fn get_context(&self) -> Option<&str> {
    None
  }

  fn resource_identifier(&self) -> &str {
    &self.resource_identifier
  }

  fn set_request(&mut self, request: String) {
    self.options.request = request;
  }
}

impl DependencyTemplate for CommonJsRequireContextDependency {
  fn apply(
    &self,
    source: &mut TemplateReplaceSource,
    code_generatable_context: &mut TemplateContext,
  ) {
    let TemplateContext {
      compilation,
      runtime_requirements,
      ..
    } = code_generatable_context;

    let expr = module_raw(
      compilation,
      runtime_requirements,
      &self.id,
      self.request(),
      false,
    );

    if compilation
      .module_graph
      .module_graph_module_by_dependency_id(&self.id)
      .is_none()
    {
      source.replace(self.callee_start, self.args_end, &expr, None);
      return;
    }

    source.replace(self.callee_start, self.callee_end, &expr, None);

    let context = normalize_context(&self.options.request);
    if !context.is_empty() {
      source.insert(self.callee_end, "(", None);
      source.insert(
        self.args_end,
        format!(".replace('{context}', './'))").as_str(),
        None,
      );
    }
  }

  fn dependency_id(&self) -> Option<DependencyId> {
    Some(self.id)
  }
}

impl AsModuleDependency for CommonJsRequireContextDependency {}
