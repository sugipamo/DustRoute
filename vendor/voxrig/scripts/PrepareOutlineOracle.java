// Development-only access fix after package remapping; no method bodies change.
// Requires ASM 9.10.1. Never distribute the resulting Minecraft JAR.
import java.nio.file.Path;
import java.util.jar.JarEntry;
import java.util.jar.JarFile;
import java.util.jar.JarOutputStream;
import java.nio.file.Files;
import org.objectweb.asm.*;

public final class PrepareOutlineOracle {
    static int access(int flags) {
        return (flags & Opcodes.ACC_PRIVATE) != 0 ? flags
            : (flags & ~Opcodes.ACC_PROTECTED) | Opcodes.ACC_PUBLIC;
    }
    public static void main(String[] args) throws Exception {
        try (var input = new JarFile(args[0]);
             var output = new JarOutputStream(Files.newOutputStream(Path.of(args[1])))) {
            for (var entries = input.entries(); entries.hasMoreElements();) {
                var entry = entries.nextElement();
                var bytes = input.getInputStream(entry).readAllBytes();
                if (entry.getName().endsWith(".class")) {
                    var writer = new ClassWriter(0);
                    new ClassReader(bytes).accept(new ClassVisitor(Opcodes.ASM9, writer) {
                        public void visit(int version, int flags, String name,
                                String signature, String parent, String[] interfaces) {
                            super.visit(version, access(flags), name, signature, parent, interfaces);
                        }
                        public void visitInnerClass(String name, String outer, String inner, int flags) {
                            super.visitInnerClass(name, outer, inner, access(flags));
                        }
                        public MethodVisitor visitMethod(int flags, String name,
                                String desc, String signature, String[] exceptions) {
                            return super.visitMethod(access(flags), name, desc, signature, exceptions);
                        }
                        public FieldVisitor visitField(int flags, String name,
                                String desc, String signature, Object value) {
                            return super.visitField(access(flags), name, desc, signature, value);
                        }
                    }, 0);
                    bytes = writer.toByteArray();
                }
                var next = new JarEntry(entry.getName());
                next.setTime(0);
                output.putNextEntry(next);
                output.write(bytes);
                output.closeEntry();
            }
        }
    }
}
