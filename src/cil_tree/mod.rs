use crate::{
    cil::CILOp,
    method::Method,
    r#type::{TyCache, Type},
};

use self::cil_root::CILRoot;
use rustc_middle::ty::TyCtxt;
/// A module containing definitions of non-root nodes of a cil tree.
pub mod cil_node;
/// A module continaing definitions of root nodess of a cil tree.
pub mod cil_root;
use serde::{Deserialize, Serialize};
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
/// A root of a CIL Tree with metadata about local variables it reads/writes into.  
pub struct CILTree {
    tree: CILRoot,
}
impl From<CILRoot> for CILTree {
    fn from(tree: CILRoot) -> Self {
        Self { tree }
    }
}
impl From<CILRoot> for Vec<CILTree> {
    fn from(tree: CILRoot) -> Self {
        vec![CILTree { tree }]
    }
}
impl CILTree {
    pub fn validate(&self, method: &Method) -> Result<(), String> {
        self.tree.validate(method)
    }
    /// Converts a `CILTree` into a list of `CILOp`.
    #[must_use]
    pub fn into_ops(&self) -> Vec<CILOp> {
        self.tree.into_ops()
    }

    pub(crate) fn fix_for_exception_handler(&mut self, id: u32) {
        self.tree.fix_for_exception_handler(id);
    }
    /// Returns a list of blocks this object may jump to.
    pub fn targets(&self, targets: &mut Vec<(u32, u32)>) {
        self.tree.targets(targets);
    }
    /// Converts a tree with subtrees into multiple trees.
    #[must_use]
    pub fn shed_trees(self) -> Vec<Self> {
        self.tree
            .shed_trees()
            .into_iter()
            .map(std::convert::Into::into)
            .collect()
    }
    /// Retunrs the root of this tree.
    #[must_use]
    pub fn root(&self) -> &CILRoot {
        &self.tree
    }
    /// Optimizes this tree
    pub fn opt(&mut self) {
        self.tree.opt();
    }
    /// Allocates the temporary variables this tree uses.
    pub(crate) fn allocate_tmps(&mut self, locals: &mut Vec<(Option<Box<str>>, Type)>) {
        self.tree.allocate_tmps(None, locals);
    }

    pub(crate) fn resolve_global_allocations(
        &mut self,
        arg: &mut crate::assembly::Assembly,
        tyctx: TyCtxt,
        tycache: &mut TyCache,
    ) {
        self.tree.resolve_global_allocations(arg, tyctx, tycache);
    }
    // TODO: remember to make this recompute tree metadtata when it is added
    pub fn root_mut(&mut self) -> &mut CILRoot {
        &mut self.tree
    }
}
/// Appends an op to a vector.
#[must_use]
pub fn append_vec(mut vec: Vec<CILOp>, by: CILOp) -> Vec<CILOp> {
    vec.push(by);
    vec
}
