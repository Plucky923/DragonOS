use core::intrinsics::unlikely;

use crate::{
    arch::{mm::frame::LockedFrameAllocator, MMArch},
    mm::{MemoryManagementArch, PhysAddr, VirtAddr},
};

/// @brief 物理页帧的表示
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PhysPageFrame {
    /// 物理页页号
    number: usize,
}

impl PhysPageFrame {
    pub fn new(paddr: PhysAddr) -> Self {
        return Self {
            number: paddr.data() / MMArch::PAGE_SIZE,
        };
    }

    /// @brief 获取当前页对应的物理地址
    pub fn phys_address(&self) -> PhysAddr {
        return PhysAddr::new(self.number * MMArch::PAGE_SIZE);
    }

    pub fn next_by(&self, n: usize) -> Self {
        return Self {
            number: self.number + n,
        };
    }

    pub fn next(&self) -> Self {
        return self.next_by(1);
    }

    /// 构造物理页帧的迭代器，范围为[start, end)
    pub fn iter_range(start: Self, end: Self) -> PhysPageFrameIter {
        return PhysPageFrameIter {
            current: start,
            end,
        };
    }
}

/// @brief 物理页帧的迭代器
#[derive(Debug)]
pub struct PhysPageFrameIter {
    current: PhysPageFrame,
    /// 结束的物理页帧（不包含）
    end: PhysPageFrame,
}

impl Iterator for PhysPageFrameIter {
    type Item = PhysPageFrame;

    fn next(&mut self) -> Option<Self::Item> {
        if unlikely(self.current == self.end) {
            return None;
        }
        let current = self.current.next();
        return Some(current);
    }
}

/// 虚拟页帧的表示
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct VirtPageFrame {
    /// 虚拟页页号
    number: usize,
}

impl VirtPageFrame {
    pub fn new(vaddr: VirtAddr) -> Self {
        return Self {
            number: vaddr.data() / MMArch::PAGE_SIZE,
        };
    }

    /// 获取当前虚拟页对应的虚拟地址
    pub fn virt_address(&self) -> VirtAddr {
        return VirtAddr::new(self.number * MMArch::PAGE_SIZE);
    }

    pub fn next_by(&self, n: usize) -> Self {
        return Self {
            number: self.number + n,
        };
    }

    pub fn next(&self) -> Self {
        return self.next_by(1);
    }

    /// 构造虚拟页帧的迭代器，范围为[start, end)
    pub fn iter_range(start: Self, end: Self) -> VirtPageFrameIter {
        return VirtPageFrameIter {
            current: start,
            end,
        };
    }
}

/// 虚拟页帧的迭代器
#[derive(Debug)]
pub struct VirtPageFrameIter {
    current: VirtPageFrame,
    /// 结束的虚拟页帧(不包含)
    end: VirtPageFrame,
}

impl Iterator for VirtPageFrameIter {
    type Item = VirtPageFrame;

    fn next(&mut self) -> Option<Self::Item> {
        if unlikely(self.current == self.end) {
            return None;
        }
        let current: VirtPageFrame = self.current.next();
        return Some(current);
    }
}

/// 页帧使用的数量
#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
pub struct PageFrameCount(usize);

impl PageFrameCount {
    // @brief 初始化PageFrameCount
    pub fn new(count: usize) -> Self {
        return Self(count);
    }
    // @brief 获取页帧数量
    pub fn data(&self) -> usize {
        return self.0;
    }
}

// 页帧使用情况
#[derive(Debug)]
pub struct PageFrameUsage {
    used: PageFrameCount,
    total: PageFrameCount,
}

impl PageFrameUsage {
    /// @brief:  初始化FrameUsage
    /// @param PageFrameCount used 已使用的页帧数量
    /// @param PageFrameCount total 总的页帧数量
    pub fn new(used: PageFrameCount, total: PageFrameCount) -> Self {
        return Self { used, total };
    }
    // @brief 获取已使用的页帧数量
    pub fn used(&self) -> PageFrameCount {
        return self.used;
    }
    // @brief 获取空闲的页帧数量
    pub fn free(&self) -> PageFrameCount {
        return PageFrameCount(self.total.0 - self.used.0);
    }
    // @brief 获取总的页帧数量
    pub fn total(&self) -> PageFrameCount {
        return self.total;
    }
}

/// 能够分配页帧的分配器需要实现的trait
pub trait FrameAllocator {
    // @brief 分配count个页帧
    unsafe fn allocate(&mut self, count: PageFrameCount) -> Option<PhysAddr>;

    // @brief 通过地址释放count个页帧
    unsafe fn free(&mut self, address: PhysAddr, count: PageFrameCount);
    // @brief 分配一个页帧
    unsafe fn allocate_one(&mut self) -> Option<PhysAddr> {
        return self.allocate(PageFrameCount::new(1));
    }
    // @brief 通过地址释放一个页帧
    unsafe fn free_one(&mut self, address: PhysAddr) {
        return self.free(address, PageFrameCount::new(1));
    }
    // @brief 获取页帧使用情况
    unsafe fn usage(&self) -> PageFrameUsage;
}

/// @brief 通过一个 &mut T 的引用来对一个实现了 FrameAllocator trait 的类型进行调用，使代码更加灵活
impl<T: FrameAllocator> FrameAllocator for &mut T {
    unsafe fn allocate(&mut self, count: PageFrameCount) -> Option<PhysAddr> {
        return T::allocate(self, count);
    }
    unsafe fn free(&mut self, address: PhysAddr, count: PageFrameCount) {
        return T::free(self, address, count);
    }
    unsafe fn allocate_one(&mut self) -> Option<PhysAddr> {
        return T::allocate_one(self);
    }
    unsafe fn free_one(&mut self, address: PhysAddr) {
        return T::free_one(self, address);
    }
    unsafe fn usage(&self) -> PageFrameUsage {
        return T::usage(self);
    }
}

/// @brief 从全局的页帧分配器中分配连续count个页帧
///
/// @param count 请求分配的页帧数量
pub fn allocate_page_frames(count: PageFrameCount) -> Option<PhysPageFrame> {
    let frame = unsafe {
        LockedFrameAllocator
            .allocate(count)
            .map(|addr| PhysPageFrame::new(addr))?
    };
    return Some(frame);
}

/// @brief 向全局页帧分配器释放连续count个页帧
///
/// @param frame 要释放的第一个页帧
/// @param count 要释放的页帧数量
pub fn deallocate_page_frames(frame: PhysPageFrame, count: PageFrameCount) {
    unsafe {
        LockedFrameAllocator.free(frame.phys_address(), count);
    }
}