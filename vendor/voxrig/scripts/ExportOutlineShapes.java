// Native Java 1.21.11 development oracle. Minecraft code/JARs are not redistributed.
import com.google.gson.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.Bootstrap;
import net.minecraft.block.*;
import net.minecraft.block.entity.BlockEntity;
import net.minecraft.fluid.FluidState;
import net.minecraft.registry.Registries;
import net.minecraft.util.hit.HitResult;
import net.minecraft.util.math.*;
import net.minecraft.util.shape.VoxelShape;
import net.minecraft.world.*;

public final class ExportOutlineShapes {
    // Audited state-only implementations. Unknown owners must remain unsupported.
    static final Set<String> OWNERS = Set.of(
        "AbstractBlock", "AirBlock", "FluidBlock", "BubbleColumnBlock",
        "AbstractPressurePlateBlock", "AbstractRedstoneGateBlock", "AbstractTorchBlock",
        "WallTorchBlock", "WallRedstoneTorchBlock", "LeverBlock", "ButtonBlock",
        "RedstoneWireBlock", "PistonBlock", "PistonHeadBlock", "SlabBlock", "StairsBlock",
        "HorizontalConnectingBlock", "WallBlock", "FenceGateBlock", "DoorBlock",
        "TrapdoorBlock", "DaylightDetectorBlock", "TripwireBlock", "TripwireHookBlock",
        "RodBlock", "HopperBlock");
    static final Gson JSON = new GsonBuilder().disableHtmlEscaping().serializeNulls().create();
    static Class<?> owner(Class<?> type, String method, Class<?>... parameters) throws Exception {
        while (type != null) {
            try {
                type.getDeclaredMethod(method, parameters);
                return type;
            } catch (NoSuchMethodException absent) { type = type.getSuperclass(); }
        }
        throw new IllegalStateException("shape method unavailable");
    }
    static final List<List<double[]>> SHAPES = new ArrayList<>();
    static final Map<String, Integer> SHAPE_IDS = new LinkedHashMap<>();
    static int shapeId(VoxelShape shape) {
        var boxes = shape.getBoundingBoxes().stream().map(b ->
            new double[]{b.minX,b.minY,b.minZ,b.maxX,b.maxY,b.maxZ}).toList();
        return SHAPE_IDS.computeIfAbsent(JSON.toJson(boxes), key -> {
            SHAPES.add(boxes); return SHAPES.size() - 1;
        });
    }
    record World(Map<BlockPos, BlockState> cells) implements BlockView {
        public BlockState getBlockState(BlockPos p) { return cells.getOrDefault(p, Blocks.AIR.getDefaultState()); }
        public BlockEntity getBlockEntity(BlockPos p) { return null; }
        public FluidState getFluidState(BlockPos p) { return getBlockState(p).getFluidState(); }
        public int getHeight() { return 384; }
        public int getBottomY() { return -64; }
    }
    static double[] xyz(Vec3d p) { return new double[]{p.x,p.y,p.z}; }
    static int[] xyz(BlockPos p) { return new int[]{p.getX(),p.getY(),p.getZ()}; }
    record Cell(int[] position, int state) {}
    record Hit(int[] position, double[] point, String face, boolean inside) {}
    record Case(List<Cell> cells, double[] start, double[] end, Hit hit) {}
    static Case sample(Map<BlockPos,BlockState> cells, double[] start, double[] end) {
        var hit = new World(cells).raycast(new RaycastContext(
            new Vec3d(start[0],start[1],start[2]), new Vec3d(end[0],end[1],end[2]),
            RaycastContext.ShapeType.OUTLINE, RaycastContext.FluidHandling.NONE, ShapeContext.absent()));
        var ordered = cells.entrySet().stream().sorted(Map.Entry.comparingByKey())
            .map(e -> new Cell(xyz(e.getKey()),Block.getRawIdFromState(e.getValue()))).toList();
        return new Case(ordered,start,end,hit.getType() == HitResult.Type.MISS ? null :
            new Hit(xyz(hit.getBlockPos()),xyz(hit.getPos()),hit.getSide().asString(),hit.isInsideBlock()));
    }
    @SuppressWarnings({"unchecked", "rawtypes"})
    static String stateKey(int id, BlockState state) {
        var properties = new TreeMap<String,String>();
        for (var entry : state.getEntries().entrySet()) {
            var property = (net.minecraft.state.property.Property)entry.getKey();
            properties.put(property.getName(),property.name(entry.getValue()));
        }
        return id + ":" + Registries.BLOCK.getId(state.getBlock()) + ":" + String.join(",",
            properties.entrySet().stream().map(e -> e.getKey()+"="+e.getValue()).toList()) + "\n";
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.createGameVersion();
        if (!SharedConstants.getGameVersion().name().equals("1.21.11"))
            throw new IllegalStateException("requires Java 1.21.11");
        Bootstrap.initialize();
        var stateShapes = new ArrayList<int[]>();
        var blocks = new ArrayList<Map<String,Object>>();
        var representatives = new LinkedHashMap<String,BlockState>();
        long registryHash = 0xcbf29ce484222325L;
        for (var block : Registries.BLOCK) {
            var outlineOwner = owner(block.getClass(),"getOutlineShape",BlockState.class,
                BlockView.class,BlockPos.class,ShapeContext.class).getSimpleName();
            var sideOwner = owner(block.getClass(),"getRaycastShape",BlockState.class,
                BlockView.class,BlockPos.class).getSimpleName();
            boolean supported = OWNERS.contains(outlineOwner) && Set.of("AbstractBlock","HopperBlock").contains(sideOwner);
            blocks.add(Map.of("name",Registries.BLOCK.getId(block).toString(),
                "outline_owner",outlineOwner,"side_owner",sideOwner,"supported",supported));
            for (var state : block.getStateManager().getStates()) {
                int id = Block.getRawIdFromState(state);
                if (id != stateShapes.size()) throw new IllegalStateException("state registry order changed");
                for (byte b : stateKey(id,state).getBytes(StandardCharsets.UTF_8))
                    registryHash = (registryHash ^ (b & 255)) * 0x100000001b3L;
                int[] pair = null;
                if (supported) {
                    pair = new int[]{shapeId(state.getOutlineShape(EmptyBlockView.INSTANCE,BlockPos.ORIGIN)),
                        shapeId(state.getRaycastShape(EmptyBlockView.INSTANCE,BlockPos.ORIGIN))};
                    representatives.putIfAbsent(Arrays.toString(pair),state);
                }
                stateShapes.add(pair);
            }
        }
        var output = new TreeMap<String,Object>();
        output.put("minecraft","1.21.11"); output.put("registry_fnv64",Long.toUnsignedString(registryHash));
        output.put("state_shapes",stateShapes); output.put("shapes",SHAPES);
        Files.writeString(Path.of(args[0]),JSON.toJson(output)+"\n");
        Files.writeString(Path.of(args[1]),new GsonBuilder().setPrettyPrinting().create().toJson(blocks)+"\n");
        var cases = new ArrayList<Case>();
        var random = new Random(12111);
        for (var state : representatives.values()) {
            var cells = Map.of(BlockPos.ORIGIN,state);
            for (int axis = 0; axis < 3; axis++) {
                for (double offset : new double[]{0,0.03125,0.125,0.5,0.875,1,1.25}) {
                    double[] start = {offset,offset,offset}, end = start.clone();
                    start[axis]=-2; end[axis]=2;
                    cases.add(sample(cells,start,end)); cases.add(sample(cells,end,start));
                }
            }
            cases.add(sample(cells,new double[]{0.5,0.5,0.5},new double[]{2,2,2}));
            cases.add(sample(cells,new double[]{-2,-2,-2},new double[]{0,0,0}));
            for (int n=0; n<24; n++) {
                double[] start=new double[3],end=new double[3];
                for (int axis=0;axis<3;axis++) {
                    start[axis]=random.nextDouble()*4-1.5; end[axis]=random.nextDouble()*4-1.5;
                }
                cases.add(sample(cells,start,end));
            }
        }
        var stone=Blocks.STONE.getDefaultState();
        var cells=Map.of(new BlockPos(0,0,1),stone,new BlockPos(0,1,0),stone,
            new BlockPos(1,0,0),stone,new BlockPos(-1,-1,-1),stone);
        for (double[] end : List.of(new double[]{3,3,3},new double[]{3,3,0.5},new double[]{0.5,3,3},
                new double[]{3,0.5,3},new double[]{-2,-2,-2},new double[]{0.5,0.5,0.5}))
            cases.add(sample(cells,new double[]{0.5,0.5,0.5},end));
        // Epsilon comparisons happen in world coordinates, including far builds.
        var lever=Blocks.LEVER.getDefaultState();
        var box=lever.getOutlineShape(EmptyBlockView.INSTANCE,BlockPos.ORIGIN).getBoundingBox();
        for (int shift : new int[]{0,100,42000,29999980}) {
            var position=new BlockPos(shift,180,shift);
            for (double edge : new double[]{shift+box.minX,shift+box.maxX}) {
                for (double epsilon : new double[]{-1e-7,0,1e-7}) {
                    double x=edge+epsilon, y=180+(box.minY+box.maxY)/2;
                    cases.add(sample(Map.of(position,lever),new double[]{x,y,shift-2},new double[]{x,y,shift+2}));
                }
            }
        }
        Files.writeString(Path.of(args[2]),JSON.toJson(cases)+"\n");
        // Pure Entity rotation math; no game world or entity simulation is run.
        var entity = new net.minecraft.entity.decoration.ArmorStandEntity(net.minecraft.entity.EntityType.ARMOR_STAND,null);
        var rotations = new ArrayList<Object>();
        for (int yaw=-128;yaw<128;yaw++) {
            for (float pitch : new float[]{-90,-89.5f,-45,-1,0,1,45,89.5f,90}) {
                float degrees = yaw * 360.0f / 256.0f;
                rotations.add(Map.of("rotation",new float[]{degrees,pitch},
                    "direction",xyz(entity.getRotationVector(pitch,degrees))));
            }
        }
        Files.writeString(Path.of(args[3]),JSON.toJson(rotations)+"\n");
        System.out.println("states="+stateShapes.size()+" supported="+stateShapes.stream().filter(Objects::nonNull).count()
            +" shapes="+SHAPES.size()+" cases="+cases.size());
    }
}
