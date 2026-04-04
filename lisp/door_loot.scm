(cond
    ((eq? material 'metal) (flatten (replicate width (standard-drops '(metal_shard)))))
    ((eq? material 'wood) (points-drops '((stick 2) (plank 3)) (* 6 width)))
    (else (display "no loot for door: ") (display material) (newline)))
