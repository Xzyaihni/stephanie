(define (try-put-furniture pos t)
    (big-combine-markers this-chunk pos t))

(define (chair-list side)
    (list
        'furniture
        'wood_chair
        side
        '(0.0 0.25 0.0) '(-0.2 0.2 0.0) '(0.2 0.15 0.0) '(0.0 0.0 0.0)))

(define (potted-plant-list side)
    (list
        'furniture
        'potted_plant
        side
        '(0.0 -0.2 0.0) '(0.35 0.15 0.0) '(-0.4 0.1 0.0) '(0.0 0.5 0.0)))

(define (generate-room-with-furniture room-seed wall-areas furnitures)
    (if (null? furnitures)
        '()
        (let
            (
                (wall-areas-length (length wall-areas))
                (total-area (fold + 0 (map (lambda (wall-area) (area-area (cdr wall-area))) wall-areas))))
            (let ((selected-area-index (random-integer-seeded (seed-with room-seed 123) wall-areas-length)))
                (let
                    (
                        (inside-index
                            (random-integer-seeded (seed-with room-seed 2) (area-area (cdr (list-ref wall-areas selected-area-index))))))
                    (loop
                        (lambda (acc)
                            (let
                                (
                                    (inside-index (list-ref acc 0))
                                    (selected-area-index (list-ref acc 1))
                                    (furnitures (list-ref acc 2)))
                                (let ((selected-area (list-ref wall-areas selected-area-index)))
                                    (let
                                        ((current-area (cdr selected-area)))
                                        (let
                                            (
                                                (place-success
                                                    ((car furnitures)
                                                        inside-index
                                                        (car selected-area)
                                                        current-area)))
                                            (let ((furnitures-tail (if place-success (cdr furnitures) furnitures)))
                                                (if (null? furnitures-tail)
                                                    '()
                                                    (if (= (+ inside-index 1) (area-area current-area))
                                                        (list
                                                            0
                                                            (if (= (+ selected-area-index 1) wall-areas-length) 0 (+ selected-area-index 1))
                                                            furnitures-tail)
                                                        (list
                                                            (+ inside-index 1)
                                                            selected-area-index
                                                            furnitures-tail)))))))))
                        (list inside-index selected-area-index furnitures)))))))

;(define (mark-room-areas wall-areas middle-area)
;    (for-each
;        (lambda (wall-area)
;            (big-fill-area
;                this-chunk
;                (cdr wall-area)
;                (let
;                    ((s (car wall-area)))
;                    (tile (cond
;                        ((= s side-up) 'grassie)
;                        ((= s side-down) 'asphalt)
;                        ((= s side-left) 'wood)
;                        ((= s side-right) 'soil)
;                        (else (display "bugy")))))))
;        wall-areas)
;    (if (not (null? middle-area))
;        (big-fill-area this-chunk middle-area (tile 'brick-path))))

(define (generate-bathroom-generic room-seed wall-areas middle-area never-cabinet)
    ;(mark-room-areas wall-areas '() (tile 'asphalt))
    (generate-room-with-furniture
        (seed-with room-seed 5)
        wall-areas
        (list
            (lambda (inside-index outer-side current-area)
                (big-put-tile
                    this-chunk
                    (area-index current-area inside-index)
                    (cons
                        'marker
                        (cons
                            (list 'furniture 'sink outer-side '(0.0 0.3 0.0) '(0.0 0.0 0.0))
                            (if (or never-cabinet (difficulty-chance 0.5 0.0))
                                '()
                                (cons (list 'furniture 'cabinet outer-side '(0.0 0.0 0.0) '(0.0 -0.3 0.0) '(0.0 -0.3 0.0)) '())))))
                #t))))

; (define (generate-bathroom room-seed wall-areas middle-area) (mark-room-areas wall-areas middle-area))
