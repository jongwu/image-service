// Copyright 2020 Ant Group. All rights reserved.
// Copyright (C) 2021-2022 Alibaba Cloud. All rights reserved.
//
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{Debug, Formatter};
use std::mem::size_of;
use std::ops::Deref;
use std::sync::Arc;

use nydus_utils::digest::RafsDigest;

use crate::metadata::cached_v5::CachedInodeV5;
use crate::metadata::chunk::ChunkWrapper;
use crate::metadata::direct_v5::OndiskInodeWrapper as OndiskInodeWrapperV5;
use crate::metadata::direct_v6::OndiskInodeWrapper as OndiskInodeWrapperV6;
use crate::metadata::layout::v5::{RafsV5ChunkInfo, RafsV5Inode, RafsV5InodeFlags};
use crate::metadata::layout::v6::{RafsV6InodeCompact, RafsV6InodeExtended, RafsV6OndiskInode};
use crate::metadata::layout::RafsXAttrs;
use crate::metadata::{Inode, RafsVersion};
use crate::RafsInodeExt;

/// An inode object wrapper for different RAFS versions.
#[derive(Clone)]
pub enum InodeWrapper {
    /// Inode info structure for RAFS v5.
    V5(RafsV5Inode),
    /// Inode info structure for RAFS v6, reuse `RafsV5Inode` as IR for v6.
    V6(RafsV5Inode),
    /// A reference to a `RafsInodeExt` object.
    Ref(Arc<dyn RafsInodeExt>),
}

impl Debug for InodeWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V5(i) => write!(f, "{:?}", i),
            Self::V6(i) => write!(f, "{:?}", i),
            Self::Ref(i) => {
                let i = to_rafsv5_inode(i.deref());
                write!(f, "{:?}", i)
            }
        }
    }
}

impl InodeWrapper {
    /// Create a new instance of `InodeWrapper` with default value.
    pub fn new(version: RafsVersion) -> Self {
        match version {
            RafsVersion::V5 => InodeWrapper::V5(RafsV5Inode::new()),
            RafsVersion::V6 => InodeWrapper::V6(RafsV5Inode::new()),
        }
    }

    /// Create an `InodeWrapper` object from a `RafsInodeExt` trait object.
    pub fn from_inode_info(inode: Arc<dyn RafsInodeExt>) -> Self {
        Self::Ref(inode)
    }

    /// Check whether is a RAFS V5 inode.
    pub fn is_v5(&self) -> bool {
        match self {
            InodeWrapper::V5(_i) => true,
            InodeWrapper::V6(_i) => false,
            InodeWrapper::Ref(inode) => {
                if let Some(_inode) = inode.as_any().downcast_ref::<CachedInodeV5>() {
                    true
                } else {
                    inode
                        .as_any()
                        .downcast_ref::<OndiskInodeWrapperV5>()
                        .is_some()
                }
            }
        }
    }

    /// Check whether is a RAFS V6 inode.
    pub fn is_v6(&self) -> bool {
        match self {
            InodeWrapper::V5(_i) => true,
            InodeWrapper::V6(_i) => false,
            InodeWrapper::Ref(inode) => inode
                .as_any()
                .downcast_ref::<OndiskInodeWrapperV6>()
                .is_some(),
        }
    }

    /// Get file content size of the inode.
    pub fn inode_size(&self) -> usize {
        match self {
            InodeWrapper::V5(i) => i.size(),
            _ => panic!("only available for RAFS v5 inode"),
        }
    }

    /// Get access permission/mode for the inode.
    pub fn mode(&self) -> u32 {
        match self {
            InodeWrapper::V5(i) => i.mode(),
            InodeWrapper::V6(i) => i.mode(),
            InodeWrapper::Ref(i) => i.get_attr().mode,
        }
    }

    /// Set access permission/mode for the inode.
    pub fn set_mode(&mut self, mode: u32) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_mode = mode,
            InodeWrapper::V6(i) => i.i_mode = mode,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Check whether the inode is a directory.
    pub fn is_dir(&self) -> bool {
        match self {
            InodeWrapper::V5(i) => i.is_dir(),
            InodeWrapper::V6(i) => i.is_dir(),
            InodeWrapper::Ref(i) => i.is_dir(),
        }
    }

    /// Check whether the inode is a regular file.
    pub fn is_reg(&self) -> bool {
        match self {
            InodeWrapper::V5(i) => i.is_reg(),
            InodeWrapper::V6(i) => i.is_reg(),
            InodeWrapper::Ref(i) => i.is_reg(),
        }
    }

    /// Check whether the inode is a hardlink.
    pub fn is_hardlink(&self) -> bool {
        match self {
            InodeWrapper::V5(i) => i.is_hardlink(),
            InodeWrapper::V6(i) => i.is_hardlink(),
            InodeWrapper::Ref(i) => i.is_hardlink(),
        }
    }

    /// Check whether the inode is a symlink.
    pub fn is_symlink(&self) -> bool {
        match self {
            InodeWrapper::V5(i) => i.is_symlink(),
            InodeWrapper::V6(i) => i.is_symlink(),
            InodeWrapper::Ref(i) => i.is_symlink(),
        }
    }

    /// Check whether the inode is a char device node.
    pub fn is_chrdev(&self) -> bool {
        match self {
            InodeWrapper::V5(i) => i.is_chrdev(),
            InodeWrapper::V6(i) => i.is_chrdev(),
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    /// Check whether the inode is a block device node.
    pub fn is_blkdev(&self) -> bool {
        match self {
            InodeWrapper::V5(i) => i.is_blkdev(),
            InodeWrapper::V6(i) => i.is_blkdev(),
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    /// Check whether the inode is a FIFO.
    pub fn is_fifo(&self) -> bool {
        match self {
            InodeWrapper::V5(i) => i.is_fifo(),
            InodeWrapper::V6(i) => i.is_fifo(),
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    /// Check whether the inode is a socket.
    pub fn is_sock(&self) -> bool {
        match self {
            InodeWrapper::V5(i) => i.is_sock(),
            InodeWrapper::V6(i) => i.is_sock(),
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    /// Check whether the inode is a special file, such chardev, blkdev, FIFO and socket.
    pub fn is_special(&self) -> bool {
        self.is_chrdev() || self.is_blkdev() || self.is_fifo() || self.is_sock()
    }

    /// Get inode flags.
    pub fn has_hardlink(&self) -> bool {
        match self {
            InodeWrapper::V5(i) => i.has_hardlink(),
            InodeWrapper::V6(i) => i.has_hardlink(),
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    /// Check whether the inode has associated xattrs.
    pub fn has_xattr(&self) -> bool {
        match self {
            InodeWrapper::V5(i) => i.has_xattr(),
            InodeWrapper::V6(i) => i.has_xattr(),
            InodeWrapper::Ref(i) => i.has_xattr(),
        }
    }

    /// Set whether the inode has associated xattrs.
    pub fn set_has_xattr(&mut self, enable: bool) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => {
                if enable {
                    i.i_flags |= RafsV5InodeFlags::XATTR;
                } else {
                    i.i_flags &= !RafsV5InodeFlags::XATTR;
                }
            }
            InodeWrapper::V6(i) => {
                if enable {
                    i.i_flags |= RafsV5InodeFlags::XATTR;
                } else {
                    i.i_flags &= !RafsV5InodeFlags::XATTR;
                }
            }
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Get inode number.
    pub fn ino(&self) -> Inode {
        match self {
            InodeWrapper::V5(i) => i.i_ino,
            InodeWrapper::V6(i) => i.i_ino,
            InodeWrapper::Ref(i) => i.ino(),
        }
    }

    /// Set inode number.
    pub fn set_ino(&mut self, ino: Inode) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_ino = ino,
            InodeWrapper::V6(i) => i.i_ino = ino,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Get parent inode number.
    pub fn parent(&self) -> Inode {
        match self {
            InodeWrapper::V5(i) => i.i_parent,
            InodeWrapper::V6(i) => i.i_parent,
            InodeWrapper::Ref(i) => i.parent(),
        }
    }

    /// Set parent inode number.
    pub fn set_parent(&mut self, parent: Inode) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_parent = parent,
            InodeWrapper::V6(i) => i.i_parent = parent,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Set inode content size of regular file, directory and symlink.
    pub fn size(&self) -> u64 {
        match self {
            InodeWrapper::V5(i) => i.i_size,
            InodeWrapper::V6(i) => i.i_size,
            InodeWrapper::Ref(i) => i.size(),
        }
    }

    /// Get inode content size.
    pub fn set_size(&mut self, size: u64) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_size = size,
            InodeWrapper::V6(i) => i.i_size = size,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Get user id associated with the inode.
    pub fn uid(&self) -> u32 {
        match self {
            InodeWrapper::V5(i) => i.i_uid,
            InodeWrapper::V6(i) => i.i_uid,
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    /// Set user id associated with the inode.
    pub fn set_uid(&mut self, uid: u32) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_uid = uid,
            InodeWrapper::V6(i) => i.i_uid = uid,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Get group id associated with the inode.
    pub fn gid(&self) -> u32 {
        match self {
            InodeWrapper::V5(i) => i.i_gid,
            InodeWrapper::V6(i) => i.i_gid,
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    /// Set group id associated with the inode.
    pub fn set_gid(&mut self, gid: u32) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_gid = gid,
            InodeWrapper::V6(i) => i.i_gid = gid,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Get modified time.
    pub fn mtime(&self) -> u64 {
        match self {
            InodeWrapper::V5(i) => i.i_mtime,
            InodeWrapper::V6(i) => i.i_mtime,
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    /// Set modified time.
    pub fn set_mtime(&mut self, mtime: u64) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_mtime = mtime,
            InodeWrapper::V6(i) => i.i_mtime = mtime,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Get nsec part of modified time.
    pub fn mtime_nsec(&self) -> u32 {
        match self {
            InodeWrapper::V5(i) => i.i_mtime_nsec,
            InodeWrapper::V6(i) => i.i_mtime_nsec,
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    /// Set nsec part of modified time.
    pub fn set_mtime_nsec(&mut self, mtime_nsec: u32) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_mtime_nsec = mtime_nsec,
            InodeWrapper::V6(i) => i.i_mtime_nsec = mtime_nsec,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Get data blocks of file content, in unit of 512 bytes.
    pub fn blocks(&self) -> u64 {
        match self {
            InodeWrapper::V5(i) => i.i_blocks,
            InodeWrapper::V6(i) => i.i_blocks,
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    /// Set data blocks of file content, in unit of 512 bytes.
    pub fn set_blocks(&mut self, blocks: u64) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_blocks = blocks,
            InodeWrapper::V6(i) => i.i_blocks = blocks,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Get real device id associated with the inode.
    pub fn rdev(&self) -> u32 {
        match self {
            InodeWrapper::V5(i) => i.i_rdev,
            InodeWrapper::V6(i) => i.i_rdev,
            InodeWrapper::Ref(i) => i.rdev(),
        }
    }

    /// Set real device id associated with the inode.
    pub fn set_rdev(&mut self, rdev: u32) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_rdev = rdev,
            InodeWrapper::V6(i) => i.i_rdev = rdev,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Set project ID associated with the inode.
    pub fn set_projid(&mut self, projid: u32) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_projid = projid,
            InodeWrapper::V6(i) => i.i_projid = projid,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Get number of hardlinks.
    pub fn nlink(&self) -> u32 {
        match self {
            InodeWrapper::V5(i) => i.i_nlink,
            InodeWrapper::V6(i) => i.i_nlink,
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    /// Set number of hardlinks.
    pub fn set_nlink(&mut self, nlink: u32) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_nlink = nlink,
            InodeWrapper::V6(i) => i.i_nlink = nlink,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Get digest of inode metadata, RAFS v5 only.
    pub fn digest(&self) -> &RafsDigest {
        match self {
            InodeWrapper::V5(i) => &i.i_digest,
            InodeWrapper::V6(i) => &i.i_digest,
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    /// Set digest of inode metadata, RAFS v5 only.
    pub fn set_digest(&mut self, digest: RafsDigest) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_digest = digest,
            InodeWrapper::V6(i) => i.i_digest = digest,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Get size of inode name.
    pub fn name_size(&self) -> u16 {
        match self {
            InodeWrapper::V5(i) => i.i_name_size,
            InodeWrapper::V6(i) => i.i_name_size,
            InodeWrapper::Ref(i) => i.get_name_size(),
        }
    }

    /// Set size of inode name.
    pub fn set_name_size(&mut self, size: usize) {
        self.ensure_owned();
        debug_assert!(size < u16::MAX as usize);
        match self {
            InodeWrapper::V5(i) => i.i_name_size = size as u16,
            InodeWrapper::V6(i) => i.i_name_size = size as u16,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Get size of symlink.
    pub fn symlink_size(&self) -> u16 {
        match self {
            InodeWrapper::V5(i) => i.i_symlink_size,
            InodeWrapper::V6(i) => i.i_symlink_size,
            InodeWrapper::Ref(i) => i.get_symlink_size(),
        }
    }

    /// Set size of symlink.
    pub fn set_symlink_size(&mut self, size: usize) {
        debug_assert!(size <= u16::MAX as usize);
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => {
                i.i_flags |= RafsV5InodeFlags::SYMLINK;
                i.i_symlink_size = size as u16;
            }
            InodeWrapper::V6(i) => {
                i.i_flags |= RafsV5InodeFlags::SYMLINK;
                i.i_symlink_size = size as u16;
            }
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Get child inode index.
    pub fn child_index(&self) -> u32 {
        match self {
            InodeWrapper::V5(i) => i.i_child_index,
            InodeWrapper::V6(i) => i.i_child_index,
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    /// Set child inode index.
    pub fn set_child_index(&mut self, index: u32) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_child_index = index,
            InodeWrapper::V6(i) => i.i_child_index = index,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Get child/chunk count.
    pub fn child_count(&self) -> u32 {
        match self {
            InodeWrapper::V5(i) => i.i_child_count,
            InodeWrapper::V6(i) => i.i_child_count,
            InodeWrapper::Ref(i) => i.get_child_count(),
        }
    }

    /// Set child/chunk count.
    pub fn set_child_count(&mut self, count: u32) {
        self.ensure_owned();
        match self {
            InodeWrapper::V5(i) => i.i_child_count = count,
            InodeWrapper::V6(i) => i.i_child_count = count,
            InodeWrapper::Ref(_i) => panic!("unexpected"),
        }
    }

    /// Create a `ChunkWrapper` object to be associated with the inode.
    pub fn create_chunk(&self) -> ChunkWrapper {
        match self {
            InodeWrapper::V5(_) => ChunkWrapper::V5(RafsV5ChunkInfo::new()),
            InodeWrapper::V6(_) => ChunkWrapper::V6(RafsV5ChunkInfo::new()),
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    /// Get memory/disk space occupied by the inode structure, including xattrs.
    pub fn get_inode_size_with_xattr(&self, xattrs: &RafsXAttrs, v6_compact: bool) -> usize {
        match self {
            InodeWrapper::V5(_i) => size_of::<RafsV5Inode>() + xattrs.aligned_size_v5(),
            InodeWrapper::V6(_i) => {
                let inode_size = if v6_compact {
                    size_of::<RafsV6InodeCompact>()
                } else {
                    size_of::<RafsV6InodeExtended>()
                };
                inode_size + xattrs.aligned_size_v6()
            }
            InodeWrapper::Ref(_i) => unimplemented!(),
        }
    }

    fn ensure_owned(&mut self) {
        if let Self::Ref(i) = self {
            let i = i.clone();
            if self.is_v6() {
                *self = Self::V6(to_rafsv5_inode(i.deref().deref()));
            } else if self.is_v5() {
                *self = Self::V5(to_rafsv5_inode(i.deref().deref()));
            } else {
                panic!("inode is neither v5 nor v6");
            }
        }
    }
}

/// Construct a `RafsV5Inode` object from a `Arc<dyn RafsInode>` object.
fn to_rafsv5_inode(inode: &dyn RafsInodeExt) -> RafsV5Inode {
    let attr = inode.get_attr();

    RafsV5Inode {
        i_digest: inode.get_digest(),
        i_parent: inode.parent(),
        i_ino: attr.ino,
        i_uid: attr.uid,
        i_gid: attr.gid,
        i_projid: inode.projid(),
        i_mode: attr.mode,
        i_size: attr.size,
        i_blocks: attr.blocks,
        i_flags: RafsV5InodeFlags::from_bits_truncate(inode.flags()),
        i_nlink: attr.nlink,
        i_child_index: inode.get_child_index().unwrap_or(0),
        i_child_count: inode.get_child_count(),
        i_name_size: inode.get_name_size(),
        i_symlink_size: inode.get_symlink_size(),
        i_rdev: attr.rdev,
        i_mtime_nsec: attr.mtimensec,
        i_mtime: attr.mtime,
        i_reserved: [0u8; 8],
    }
}

/// Create RAFS v6 on-disk inode object.
pub fn new_v6_inode(
    inode: &InodeWrapper,
    datalayout: u16,
    xattr_inline_count: u16,
    compact: bool,
) -> Box<dyn RafsV6OndiskInode> {
    let mut i: Box<dyn RafsV6OndiskInode> = match compact {
        true => Box::new(RafsV6InodeCompact::new()),
        false => Box::new(RafsV6InodeExtended::new()),
    };

    assert!(inode.ino() <= i32::MAX as Inode);
    i.set_ino(inode.ino() as u32);
    i.set_size(inode.size());
    i.set_uidgid(inode.uid(), inode.gid());
    i.set_mtime(inode.mtime(), inode.mtime_nsec());
    i.set_nlink(inode.nlink());
    i.set_mode(inode.mode() as u16);
    i.set_data_layout(datalayout);
    i.set_xattr_inline_count(xattr_inline_count);
    if inode.is_special() {
        i.set_rdev(inode.rdev() as u32);
    }

    i
}
