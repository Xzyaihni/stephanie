(define (generate-roof) (filled-chunk (tile 'concrete)))

(define (generate-floor) (filled-chunk (tile 'concrete-path)))

(define (generate-ground)
    (define this-chunk (filled-chunk (tile 'concrete-path)))
    (put-tile
        this-chunk
        (make-point 3 6)
        (tile 'stairs-down (side-combine side-down rotation))))

(define (generate-walls)
    (if (= height 1)
        (begin
            (define this-chunk (filled-chunk (tile 'concrete)))
            (fill-area
                this-chunk
                (make-area
                    (make-point 1 1)
                    (make-point 6 6))
                (tile 'air))
            (put-tile
                this-chunk
                (make-point 4 4)
                (single-marker (list 'light 1.3)))
            (let ((doorway-point (make-point 4 0)))
                (begin
                    (fill-area
                        this-chunk
                        (make-area
                            doorway-point
                            (make-point 2 1))
                        (tile 'air))
                    (put-tile
                        this-chunk
                        doorway-point
                        (single-marker (list 'door side-left 'metal 2))))))
        (begin
            (define this-chunk (filled-chunk (tile 'concrete)))
            (define (place-furniture point name side)
                (combine-markers this-chunk point (list 'furniture name side)))
            (fill-area
                this-chunk
                (make-area
                    (make-point 1 1)
                    (make-point 6 6))
                (tile 'air))
            (put-tile
                this-chunk
                (make-point 3 4)
                (single-marker (list 'light 1.3)))
            (place-furniture (make-point 5 4) 'wood_table side-left)
            (let ((chair-index (random-integer 4)))
                (cond
                    ((= chair-index 0) (place-furniture (make-point 6 3) 'wood_chair side-up))
                    ((= chair-index 1) (place-furniture (make-point 5 3) 'wood_chair side-up))
                    ((= chair-index 2) (place-furniture (make-point 6 5) 'wood_chair side-down))
                    (else (place-furniture (make-point 5 5) 'wood_chair side-down))))
            (place-furniture (make-point 1 1) 'bed (if (random-bool) side-up side-left))
            (place-furniture (make-point 6 1) 'sink side-right)
            (if (> difficulty 0.0)
                (combine-markers this-chunk (make-point 4 2) '(enemy me)))
            (put-tile
                this-chunk
                (make-point 3 6)
                (tile 'stairs-up (side-combine side-down rotation))))))

(if (= height 2)
    (generate-roof)
    (if (or (= height 1) (= height -1))
        (generate-walls)
        (if (= height 0)
            (generate-ground)
            (generate-floor))))
