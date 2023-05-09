use core::{
    fmt::{self, Debug},
    marker::PhantomData,
    mem,
    ops::Add,
};

use crate::{arch::MMArch, kerror};

use super::{
    allocator::page_frame::FrameAllocator, MemoryManagementArch, PageTableKind, PhysAddr,
    PhysMemoryArea, VirtAddr,
};

pub struct PageTable<Arch> {
    /// 当前页表表示的虚拟地址空间的起始地址
    base: VirtAddr,
    /// 当前页表所在的物理地址
    phys: PhysAddr,
    /// 当前页表的层级（请注意，最顶级页表的level为[Arch::PAGE_LEVELS - 1]）
    level: usize,
    phantom: PhantomData<Arch>,
}

impl<Arch: MemoryManagementArch> PageTable<Arch> {
    pub unsafe fn new(base: VirtAddr, phys: PhysAddr, level: usize) -> Self {
        Self {
            base,
            phys,
            level,
            phantom: PhantomData,
        }
    }

    /// @brief 获取顶级页表
    ///
    /// @param table_kind 页表类型
    ///
    /// @return 顶级页表
    pub unsafe fn top_level_table(table_kind: PageTableKind) -> Self {
        return Self::new(
            VirtAddr::new(0),
            Arch::table(table_kind),
            Arch::PAGE_LEVELS - 1,
        );
    }

    /// @brief 获取当前页表的物理地址
    #[inline(always)]
    pub fn phys(&self) -> PhysAddr {
        self.phys
    }

    /// @brief 获取当前页表表示的内存空间的起始地址
    #[inline(always)]
    pub fn base(&self) -> VirtAddr {
        self.base
    }

    /// @brief 获取当前页表的层级
    #[inline(always)]
    pub fn level(&self) -> usize {
        self.level
    }

    /// @brief 获取当前页表自身所在的虚拟地址
    #[inline(always)]
    pub unsafe fn virt(&self) -> VirtAddr {
        return Arch::phys_2_virt(self.phys).unwrap();
    }

    /// @brief 获取第i个页表项所表示的虚拟内存空间的起始地址
    pub fn entry_base(&self, i: usize) -> Option<VirtAddr> {
        if i < Arch::PAGE_ENTRY_NUM {
            let shift = self.level * Arch::PAGE_ENTRY_SHIFT + Arch::PAGE_SHIFT;
            return Some(self.base.add(i << shift));
        } else {
            return None;
        }
    }

    /// @brief 获取当前页表的第i个页表项所在的虚拟地址（注意与entry_base进行区分）
    pub unsafe fn entry_virt(&self, i: usize) -> Option<VirtAddr> {
        if i < Arch::PAGE_ENTRY_NUM {
            return Some(self.virt().add(i * Arch::PAGE_ENTRY_SIZE));
        } else {
            return None;
        }
    }

    /// @brief 获取当前页表的第i个页表项
    pub unsafe fn entry(&self, i: usize) -> Option<PageEntry<Arch>> {
        let entry_virt = self.entry_virt(i)?;
        return Some(PageEntry::new(Arch::read::<usize>(entry_virt)));
    }

    /// @brief 设置当前页表的第i个页表项
    pub unsafe fn set_entry(&self, i: usize, entry: PageEntry<Arch>) -> Option<()> {
        let entry_virt = self.entry_virt(i)?;
        Arch::write::<usize>(entry_virt, entry.data());
        return Some(());
    }

    /// @brief 判断当前页表的第i个页表项是否已经填写了值
    ///
    /// @return Some(true) 如果已经填写了值
    /// @return Some(false) 如果未填写值
    /// @return None 如果i超出了页表项的范围
    pub fn entry_mapped(&self, i: usize) -> Option<bool> {
        let etv = unsafe { self.entry_virt(i) }?;
        if unsafe { Arch::read::<usize>(etv) } != 0 {
            return Some(true);
        } else {
            return Some(false);
        }
    }

    /// @brief 根据虚拟地址，获取对应的页表项在页表中的下标
    ///
    /// @param addr 虚拟地址
    ///
    /// @return 页表项在页表中的下标。如果addr不在当前页表所表示的虚拟地址空间中，则返回None
    pub unsafe fn index_of(&self, addr: VirtAddr) -> Option<usize> {
        let addr = VirtAddr::new(addr.data() & Arch::PAGE_ADDRESS_MASK);
        let shift = self.level * Arch::PAGE_ENTRY_SHIFT + Arch::PAGE_SHIFT;

        let index = addr.data() >> shift;
        if index >= Arch::PAGE_ENTRY_NUM {
            return None;
        }
        return Some(index & Arch::PAGE_ENTRY_MASK);
    }

    /// @brief 获取第i个页表项指向的下一级页表
    pub unsafe fn next_level_table(&self, index: usize) -> Option<Self> {
        if self.level == 0 {
            return None;
        }

        // 返回下一级页表
        return Some(PageTable::new(
            self.entry_base(index)?,
            self.entry(index)?.address().ok()?,
            self.level - 1,
        ));
    }
}

/// 页表项
#[derive(Debug, Copy, Clone)]
pub struct PageEntry<Arch> {
    data: usize,
    phantom: PhantomData<Arch>,
}

impl<Arch: MemoryManagementArch> PageEntry<Arch> {
    #[inline(always)]
    pub fn new(data: usize) -> Self {
        Self {
            data,
            phantom: PhantomData,
        }
    }

    #[inline(always)]
    pub fn data(&self) -> usize {
        self.data
    }

    /// @brief 获取当前页表项指向的物理地址
    ///
    /// @return Ok(PhysAddr) 如果当前页面存在于物理内存中, 返回物理地址
    /// @return Err(PhysAddr) 如果当前页表项不存在, 返回物理地址
    #[inline(always)]
    pub fn address(&self) -> Result<PhysAddr, PhysAddr> {
        let paddr = PhysAddr::new(self.data & Arch::PAGE_ADDRESS_MASK);

        if self.present() {
            Ok(paddr)
        } else {
            Err(paddr)
        }
    }

    #[inline(always)]
    pub fn flags(&self) -> PageFlags<Arch> {
        PageFlags::new(self.data & Arch::ENTRY_FLAGS_MASK)
    }

    #[inline(always)]
    pub fn set_flags(&mut self, flags: PageFlags<Arch>) {
        self.data = (self.data & !Arch::ENTRY_FLAGS_MASK) | flags.data();
    }

    #[inline(always)]
    pub fn present(&self) -> bool {
        return self.data & Arch::ENTRY_FLAG_PRESENT != 0;
    }
}

/// 页表项的标志位
#[derive(Copy, Clone, Hash)]
pub struct PageFlags<Arch> {
    data: usize,
    phantom: PhantomData<Arch>,
}

impl<Arch: MemoryManagementArch> PageFlags<Arch> {
    #[inline(always)]
    pub fn new(data: usize) -> Self {
        return unsafe { Self::from_data(data) };
    }

    #[inline(always)]
    pub fn data(&self) -> usize {
        self.data
    }

    #[inline(always)]
    pub unsafe fn from_data(data: usize) -> Self {
        return Self {
            data: data,
            phantom: PhantomData,
        };
    }

    /// @brief 为新页表的页表项设置默认值
    /// 默认值为：
    /// - present
    /// - read only
    /// - kernel space
    /// - no exec
    #[inline(always)]
    pub fn new_page_table() -> Self {
        return unsafe {
            Self::from_data(
                Arch::ENTRY_FLAG_DEFAULT_TABLE
                    | Arch::ENTRY_FLAG_READONLY
                    | Arch::ENTRY_FLAG_NO_EXEC,
            )
        };
    }

    /// @brief 取得当前页表项的所有权，更新当前页表项的标志位，并返回更新后的页表项。
    ///
    /// @param flag 要更新的标志位的值
    /// @param value 如果为true，那么将flag对应的位设置为1，否则设置为0
    ///
    /// @return 更新后的页表项
    #[inline(always)]
    #[must_use]
    pub fn update_flags(mut self, flag: usize, value: bool) -> Self {
        if value {
            self.data |= flag;
        } else {
            self.data &= !flag;
        }
        return self;
    }

    /// @brief 判断当前页表项是否存在指定的flag（只有全部flag都存在才返回true）
    #[inline(always)]
    pub fn has_flag(&self, flag: usize) -> bool {
        return self.data & flag == flag;
    }

    #[inline(always)]
    pub fn present(&self) -> bool {
        return self.has_flag(Arch::ENTRY_FLAG_PRESENT);
    }

    /// @brief 设置当前页表项的权限
    ///
    /// @param value 如果为true，那么将当前页表项的权限设置为用户态可访问
    #[must_use]
    #[inline(always)]
    pub fn set_user(self, value: bool) -> Self {
        return self.update_flags(Arch::ENTRY_FLAG_USER, value);
    }

    /// @brief 用户态是否可以访问当前页表项
    #[inline(always)]
    pub fn user(&self) -> bool {
        return self.has_flag(Arch::ENTRY_FLAG_USER);
    }

    /// @brief 设置当前页表项的可写性, 如果为true，那么将当前页表项的权限设置为可写, 否则设置为只读
    ///
    /// @return 更新后的页表项. 请注意，本函数会取得当前页表项的所有权，因此返回的页表项不是原来的页表项
    #[must_use]
    #[inline(always)]
    pub fn set_write(self, value: bool) -> Self {
        // 有的架构同时具有可写和不可写的标志位，因此需要同时更新
        return self
            .update_flags(Arch::ENTRY_FLAG_READONLY, !value)
            .update_flags(Arch::ENTRY_FLAG_READWRITE, value);
    }

    /// @brief 当前页表项是否可写
    #[inline(always)]
    pub fn write(&self) -> bool {
        // 有的架构同时具有可写和不可写的标志位，因此需要同时判断
        return self.data & (Arch::ENTRY_FLAG_READWRITE | Arch::ENTRY_FLAG_READONLY)
            == Arch::ENTRY_FLAG_READWRITE;
    }

    /// @brief 设置当前页表项的可执行性, 如果为true，那么将当前页表项的权限设置为可执行, 否则设置为不可执行
    #[must_use]
    #[inline(always)]
    pub fn set_execute(self, value: bool) -> Self {
        // 有的架构同时具有可执行和不可执行的标志位，因此需要同时更新
        return self
            .update_flags(Arch::ENTRY_FLAG_NO_EXEC, !value)
            .update_flags(Arch::ENTRY_FLAG_EXEC, value);
    }

    /// @brief 当前页表项是否可执行
    #[inline(always)]
    pub fn execute(&self) -> bool {
        // 有的架构同时具有可执行和不可执行的标志位，因此需要同时判断
        return self.data & (Arch::ENTRY_FLAG_EXEC | Arch::ENTRY_FLAG_NO_EXEC)
            == Arch::ENTRY_FLAG_EXEC;
    }
}

impl<Arch: MemoryManagementArch> fmt::Debug for PageFlags<Arch> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PageFlags")
            .field("bits", &format_args!("{:#0x}", self.data))
            .field("present", &self.present())
            .field("write", &self.write())
            .field("executable", &self.execute())
            .field("user", &self.user())
            .finish()
    }
}

/// @brief 页表映射器
#[derive(Hash)]
pub struct PageMapper<Arch, F> {
    /// 页表类型
    table_kind: PageTableKind,
    /// 根页表物理地址
    table_paddr: PhysAddr,
    /// 页分配器
    frame_allocator: F,
    phantom: PhantomData<fn() -> Arch>,
}

impl<Arch: MemoryManagementArch, F: FrameAllocator> PageMapper<Arch, F> {
    /// @brief 创建新的页面映射器
    ///
    /// @param table_kind 页表类型
    /// @param table_paddr 根页表物理地址
    /// @param allocator 页分配器
    ///
    /// @return 页面映射器
    pub unsafe fn new(table_kind: PageTableKind, table_paddr: PhysAddr, allocator: F) -> Self {
        return Self {
            table_kind,
            table_paddr,
            frame_allocator: allocator,
            phantom: PhantomData,
        };
    }

    /// @brief 创建页表，并为这个页表创建页面映射器
    pub unsafe fn create(table_kind: PageTableKind, mut allocator: F) -> Option<Self> {
        let table_paddr = allocator.allocate_one()?;
        return Some(Self::new(table_kind, table_paddr, allocator));
    }

    /// @brief 获取当前页表的页面映射器
    #[inline(always)]
    pub unsafe fn current(table_kind: PageTableKind, allocator: F) -> Self {
        let table_paddr = Arch::table(table_kind);
        return Self::new(table_kind, table_paddr, allocator);
    }

    /// @brief 判断当前页表分配器所属的页表是否是当前页表
    #[inline(always)]
    pub fn is_current(&self) -> bool {
        return unsafe { self.table().phys() == Arch::table(self.table_kind) };
    }

    /// @brief 将当前页表分配器所属的页表设置为当前页表
    #[inline(always)]
    pub unsafe fn make_current(&self) {
        Arch::set_table(self.table_kind, self.table_paddr);
    }

    /// @brief 获取当前页表分配器所属的根页表的结构体
    #[inline(always)]
    pub fn table(&self) -> PageTable<Arch> {
        // 由于只能通过new方法创建PageMapper，因此这里假定table_paddr是有效的
        return unsafe {
            PageTable::new(VirtAddr::new(0), self.table_paddr, Arch::PAGE_LEVELS - 1)
        };
    }

    /// @brief 获取当前PageMapper所对应的页分配器实例的引用
    #[inline(always)]
    pub fn allocator_ref(&self) -> &F {
        return &self.frame_allocator;
    }

    /// @brief 获取当前PageMapper所对应的页分配器实例的可变引用
    #[inline(always)]
    pub fn allocator_mut(&mut self) -> &mut F {
        return &mut self.frame_allocator;
    }

    /// @brief 从当前PageMapper的页分配器中分配一个物理页，并将其映射到指定的虚拟地址
    pub unsafe fn map(
        &mut self,
        virt: VirtAddr,
        flags: PageFlags<Arch>,
    ) -> Option<PageFlush<Arch>> {
        let phys: PhysAddr = self.frame_allocator.allocate_one()?;
        return self.map_phys(virt, phys, flags);
    }

    /// @brief 映射一个物理页到指定的虚拟地址
    pub unsafe fn map_phys(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: PageFlags<Arch>,
    ) -> Option<PageFlush<Arch>> {
        // 验证虚拟地址和物理地址是否对齐
        if !(virt.check_aligned(Arch::PAGE_SIZE) && phys.check_aligned(Arch::PAGE_SIZE)) {
            kerror!(
                "Try to map unaligned page: virt={:?}, phys={:?}",
                virt,
                phys
            );
            return None;
        }

        // TODO： 验证flags是否合法

        // 创建页表项
        let entry = PageEntry::new(phys.data() | flags.data());
        let mut table = self.table();

        loop {
            let i = table.index_of(virt)?;
            if table.level() == 0 {
                // 检查是否已经映射
                if table.entry_mapped(i)? == true {
                    panic!("Page {:?} already mapped", virt);
                }
                table.set_entry(i, entry);
                return Some(PageFlush::new(virt));
            } else {
                let next_table = table.next_level_table(i);
                if let Some(next_table) = next_table {
                    table = next_table;
                } else {
                    // 分配下一级页表
                    let frame = self.frame_allocator.allocate_one()?;
                    // 设置页表项的flags
                    let flags = Arch::ENTRY_FLAG_READWRITE
                        | Arch::ENTRY_FLAG_DEFAULT_TABLE
                        | if virt.kind() == PageTableKind::User {
                            Arch::ENTRY_FLAG_USER
                        } else {
                            0
                        };
                    // 把新分配的页表映射到当前页表
                    table.set_entry(i, PageEntry::new(frame.data() | flags));
                    // 获取新分配的页表
                    table = table.next_level_table(i)?;
                }
            }
        }
    }

    /// @brief 将物理地址映射到具有线性偏移量的虚拟地址
    pub unsafe fn map_linearly(
        &mut self,
        phys: PhysAddr,
        flags: PageFlags<Arch>,
    ) -> Option<(VirtAddr, PageFlush<Arch>)> {
        let virt: VirtAddr = Arch::phys_2_virt(phys)?;
        return self.map_phys(virt, phys, flags).map(|flush| (virt, flush));
    }

    /// @brief 修改虚拟地址的页表项的flags，并返回页表项刷新器
    ///
    /// 请注意，需要在修改完flags后，调用刷新器的flush方法，才能使修改生效
    ///
    /// @param virt 虚拟地址
    /// @param flags 新的页表项的flags
    ///
    /// @return 如果修改成功，返回刷新器，否则返回None
    pub unsafe fn remap(
        &mut self,
        virt: VirtAddr,
        flags: PageFlags<Arch>,
    ) -> Option<PageFlush<Arch>> {
        return self
            .visit(virt, |p1, i| {
                let mut entry = p1.entry(i)?;
                entry.set_flags(flags);
                p1.set_entry(i, entry);
                Some(PageFlush::new(virt))
            })
            .flatten();
    }

    /// @brief 根据虚拟地址，查找页表，获取对应的物理地址和页表项的flags
    ///
    /// @param virt 虚拟地址
    ///
    /// @return 如果查找成功，返回物理地址和页表项的flags，否则返回None
    pub fn translate(&self, virt: VirtAddr) -> Option<(PhysAddr, PageFlags<Arch>)> {
        let entry: PageEntry<Arch> = self.visit(virt, |p1, i| unsafe { p1.entry(i) }).flatten()?;
        let paddr = entry.address().ok()?;
        let flags = entry.flags();
        return Some((paddr, flags));
    }

    /// @brief 取消虚拟地址的映射，释放页面，并返回页表项刷新器
    ///
    /// 请注意，需要在取消映射后，调用刷新器的flush方法，才能使修改生效
    ///
    /// @param virt 虚拟地址
    /// @param unmap_parents 是否在父页表内，取消空闲子页表的映射
    ///
    /// @return 如果取消成功，返回刷新器，否则返回None
    pub unsafe fn unmap(&mut self, virt: VirtAddr, unmap_parents: bool) -> Option<PageFlush<Arch>> {
        let (paddr, _, flusher) = self.unmap_phys(virt, unmap_parents)?;
        self.frame_allocator.free_one(paddr);
        return Some(flusher);
    }

    /// @brief 取消虚拟地址的映射，并返回物理地址和页表项的flags
    ///
    /// @param vaddr 虚拟地址
    /// @param unmap_parents 是否在父页表内，取消空闲子页表的映射
    ///
    /// @return 如果取消成功，返回物理地址和页表项的flags，否则返回None
    pub unsafe fn unmap_phys(
        &mut self,
        virt: VirtAddr,
        unmap_parents: bool,
    ) -> Option<(PhysAddr, PageFlags<Arch>, PageFlush<Arch>)> {
        if !virt.check_aligned(Arch::PAGE_SIZE) {
            kerror!("Try to unmap unaligned page: virt={:?}", virt);
            return None;
        }

        let mut table = self.table();
        return unmap_phys_inner(virt, &mut table, unmap_parents, self.allocator_mut())
            .map(|(paddr, flags)| (paddr, flags, PageFlush::<Arch>::new(virt)));
    }

    /// @brief 在页表中，访问虚拟地址对应的页表项，并调用传入的函数F
    fn visit<T>(
        &self,
        virt: VirtAddr,
        f: impl FnOnce(&mut PageTable<Arch>, usize) -> T,
    ) -> Option<T> {
        let mut table = self.table();
        unsafe {
            loop {
                let i = table.index_of(virt)?;
                if table.level() == 0 {
                    return Some(f(&mut table, i));
                } else {
                    table = table.next_level_table(i)?;
                }
            }
        }
    }
}

/// @brief 取消页面映射，返回被取消映射的页表项的：【物理地址】和【flags】
///
/// @param vaddr 虚拟地址
/// @param table 页表
/// @param unmap_parents 是否在父页表内，取消空闲子页表的映射
/// @param allocator 页面分配器（如果页表从这个分配器分配，那么在取消映射时，也需要归还到这个分配器内）
///
/// @return 如果取消成功，返回被取消映射的页表项的：【物理地址】和【flags】，否则返回None
unsafe fn unmap_phys_inner<Arch: MemoryManagementArch>(
    vaddr: VirtAddr,
    table: &mut PageTable<Arch>,
    unmap_parents: bool,
    allocator: &mut impl FrameAllocator,
) -> Option<(PhysAddr, PageFlags<Arch>)> {
    // 获取页表项的索引
    let i = table.index_of(vaddr)?;

    // 如果当前是最后一级页表，直接取消页面映射
    if table.level() == 0 {
        let entry = table.entry(i)?;
        table.set_entry(i, PageEntry::new(0));
        return Some((entry.address().ok()?, entry.flags()));
    }

    let mut subtable = table.next_level_table(i)?;
    // 递归地取消映射
    let result = unmap_phys_inner(vaddr, &mut subtable, unmap_parents, allocator)?;

    // TODO: This is a bad idea for architectures where the kernel mappings are done in the process tables,
    // as these mappings may become out of sync
    if unmap_parents {
        // 如果子页表已经没有映射的页面了，就取消子页表的映射

        // 检查子页表中是否还有映射的页面
        let x = (0..Arch::PAGE_ENTRY_NUM)
            .map(|k| subtable.entry(k).expect("invalid page entry"))
            .any(|e| e.present());
        if !x {
            // 如果没有，就取消子页表的映射
            table.set_entry(i, PageEntry::new(0));
            // 释放子页表
            allocator.free_one(subtable.phys());
        }
    }

    return Some(result);
}

impl<Arch, F: Debug> Debug for PageMapper<Arch, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PageMapper")
            .field("table_paddr", &self.table_paddr)
            .field("frame_allocator", &self.frame_allocator)
            .finish()
    }
}

/// 页表刷新器的trait
pub trait Flusher<Arch> {
    /// 取消对指定的page flusher的刷新
    fn consume(&mut self, flush: PageFlush<Arch>);
}

/// @brief 用于刷新某个虚拟地址的刷新器。这个刷新器一经产生，就必须调用flush()方法，
/// 否则会造成对页表的更改被忽略，这是不安全的
#[must_use = "The flusher must call the 'flush()', or the changes to page table will be unsafely ignored."]
pub struct PageFlush<Arch> {
    virt: VirtAddr,
    phantom: PhantomData<Arch>,
}

impl<Arch: MemoryManagementArch> PageFlush<Arch> {
    pub fn new(virt: VirtAddr) -> Self {
        return Self {
            virt,
            phantom: PhantomData,
        };
    }

    pub fn flush(self) {
        unsafe { Arch::invalidate_page(self.virt) };
    }

    /// @brief 忽略掉这个刷新器
    pub unsafe fn ignore(self) {
        mem::forget(self);
    }
}

/// @brief 用于刷新整个页表的刷新器。这个刷新器一经产生，就必须调用flush()方法，
/// 否则会造成对页表的更改被忽略，这是不安全的
#[must_use = "The flusher must call the 'flush()', or the changes to page table will be unsafely ignored."]
pub struct PageFlushAll<Arch: MemoryManagementArch> {
    phantom: PhantomData<fn() -> Arch>,
}

impl<Arch: MemoryManagementArch> PageFlushAll<Arch> {
    pub fn new() -> Self {
        return Self {
            phantom: PhantomData,
        };
    }

    pub fn flush(self) {
        unsafe { Arch::invalidate_all() };
    }

    /// @brief 忽略掉这个刷新器
    pub unsafe fn ignore(self) {
        mem::forget(self);
    }
}

impl<Arch: MemoryManagementArch> Flusher<Arch> for PageFlushAll<Arch> {
    /// 为page flush all 实现consume，消除对单个页面的刷新。（刷新整个页表了就不需要刷新单个页面了）
    fn consume(&mut self, flush: PageFlush<Arch>) {
        unsafe { flush.ignore() };
    }
}

impl<Arch: MemoryManagementArch, T: Flusher<Arch> + ?Sized> Flusher<Arch> for &mut T {
    /// 允许一个flusher consume掉另一个flusher
    fn consume(&mut self, flush: PageFlush<Arch>) {
        <T as Flusher<Arch>>::consume(self, flush);
    }
}

impl<Arch: MemoryManagementArch> Flusher<Arch> for () {
    fn consume(&mut self, flush: PageFlush<Arch>) {}
}

/// # 把一个地址向下对齐到页大小
pub fn round_down_to_page_size(addr: usize) -> usize {
    addr & !(MMArch::PAGE_SIZE - 1)
}

/// # 把一个地址向上对齐到页大小
pub fn round_up_to_page_size(addr: usize) -> usize {
    round_down_to_page_size(addr + MMArch::PAGE_SIZE - 1)
}