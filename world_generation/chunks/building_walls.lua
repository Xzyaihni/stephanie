function(neighbors)
    local chunk = {};

    for i = 1, 16 * 16 do
        local tile = "air";

        -- i hate lua so much
        -- why does it begin indices at 1, WHY?????
        -- im rewriting this in lisp
        if (i // 16 == 0) or (i // 16 == 15) or (i % 16 == 1) or (i % 16 == 0) then
            tile = "concrete";
        end

        chunk[i] = tilemap[tile];
    end

    return chunk;
end
