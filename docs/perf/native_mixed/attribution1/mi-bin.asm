
<image>:     file format elf64-x86-64


Disassembly of section .text:

000000000005b180 <mi_bin>:
   5b180:	48 83 c7 07          	add    $0x7,%rdi
   5b184:	48 89 fa             	mov    %rdi,%rdx
   5b187:	48 c1 ea 03          	shr    $0x3,%rdx
   5b18b:	48 83 ff 47          	cmp    $0x47,%rdi
   5b18f:	77 1f                	ja     5b1b0 <mi_bin+0x30>
   5b191:	48 8d 42 01          	lea    0x1(%rdx),%rax
   5b195:	48 83 e0 fe          	and    $0xfffffffffffffffe,%rax
   5b199:	48 83 fa 01          	cmp    $0x1,%rdx
   5b19d:	ba 01 00 00 00       	mov    $0x1,%edx
   5b1a2:	48 0f 46 c2          	cmovbe %rdx,%rax
   5b1a6:	c3                   	ret
   5b1a7:	66 0f 1f 84 00 00 00 	nopw   0x0(%rax,%rax,1)
   5b1ae:	00 00 
   5b1b0:	48 81 fa 00 00 01 00 	cmp    $0x10000,%rdx
   5b1b7:	77 17                	ja     5b1d0 <mi_bin+0x50>
   5b1b9:	48 83 ea 01          	sub    $0x1,%rdx
   5b1bd:	48 0f bd c2          	bsr    %rdx,%rax
   5b1c1:	8d 48 fe             	lea    -0x2(%rax),%ecx
   5b1c4:	48 d3 ea             	shr    %cl,%rdx
   5b1c7:	83 e2 03             	and    $0x3,%edx
   5b1ca:	48 8d 44 82 fd       	lea    -0x3(%rdx,%rax,4),%rax
   5b1cf:	c3                   	ret
   5b1d0:	b8 49 00 00 00       	mov    $0x49,%eax
   5b1d5:	c3                   	ret

Disassembly of section .init:

Disassembly of section .fini:

Disassembly of section .plt:
