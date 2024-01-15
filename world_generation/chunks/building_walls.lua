function(neighbors)
    local chunk = {};

    for i = 1, 16 * 16 do
        local tile = "air";

        if (i / 16 == 0) or (i / 16 == 15) or (i % 16 == 0) or (i % 16 == 15) then
            tile = "concrete";
        end

        chunk[i] = tilemap[tile];
    end

    return chunk;
end
