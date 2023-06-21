function(neighbors)
    local chunk = {};

    local border_width = 3;

    for x = 0, 15 do
        for y = 0, 15 do
            local line = "asphalt";

            if (x < border_width or x > 15 - border_width)
                and (y < border_width or y > 15 - border_width) then
                line = "concrete";
            end

            local i = y * 16 + x + 1;

            chunk[i] = tilemap[line];
        end
    end

    return chunk;
end
