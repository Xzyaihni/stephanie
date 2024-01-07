function(neighbors)
    local chunk = {};

    for i = 1, 16 * 16 do
        local tile = "soil";

        chunk[i] = tilemap[tile];
    end

    return chunk;
end
