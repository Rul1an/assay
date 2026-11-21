// Clang emits a CO-RE field relocation at each address calculation. The
// declarations need only name the accessed members of these kernel types.
struct file { struct inode *f_inode; };
struct inode { unsigned long i_ino; unsigned i_generation; struct super_block *i_sb; };
struct super_block { unsigned s_dev; };

#define FIELD_ADDRESS(type, member) \
    const void *assay_##member(const struct type *p) { \
        return __builtin_preserve_access_index(&p->member); \
    }

FIELD_ADDRESS(file, f_inode)
FIELD_ADDRESS(inode, i_ino)
FIELD_ADDRESS(inode, i_generation)
FIELD_ADDRESS(inode, i_sb)
FIELD_ADDRESS(super_block, s_dev)
