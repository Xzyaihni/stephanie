(define (generate-chunk middle-position part)

(define building-height
    (let ((x (assq 'building-height (chunk-tags-at middle-position))))
        (if debug-mode
            (if (null? x)
                (begin
                    (if (not (allow-out-of-range-chunks)) (begin (display "building-height not found") (newline)))
                    15)
                x)
            x)))

(if (>= height building-height) (filled-chunk (tile 'air))
(begin

(define big-size-x (* size-x 2))
(define big-size-y (* size-y 2))

(define in-big-chunk-pos
    (point-zip-map
        (make-point size-x size-y)
        (cond
            ((eq? part 'bl) (make-point 0 1))
            ((eq? part 'br) (make-point 1 1))
            ((eq? part 'tl) (make-point 0 0))
            (else (make-point 1 0)))
        (lambda (x y) (* x y))))

(load "multichunk_common.scm")

(define wall-tile (tile 'concrete))

(define (put-outer-walls this-chunk)
    (big-fill-area
        (big-fill-area
            (big-fill-area
                (big-fill-area
                    this-chunk
                    (make-area (make-point 1 1) (make-point 14 1))
                    wall-tile)
                (make-area (make-point 1 2) (make-point 1 13))
                wall-tile)
            (make-area (make-point 2 14) (make-point 13 1))
            wall-tile)
        (make-area (make-point 14 2) (make-point 1 12))
        wall-tile))

(define (put-floor this-chunk)
    (big-fill-area
        (big-fill-area
            (put-outer-walls this-chunk)
            (make-area (make-point 2 2) (make-point (- big-size-x 4) (- big-size-y 4)))
            (tile 'wood))
        (make-area (make-point 5 1) (make-point 6 4))
        (tile 'concrete)))

(define roof-start (- building-height 4))

(cond
    ((> height roof-start)
        (cond
            ((= height (+ roof-start 1))
                (define this-chunk
                    (big-fill-area
                        (filled-chunk (tile 'air))
                        (make-area (make-point 1 1) (make-point 14 14))
                        (tile 'concrete)))
                (big-put-tile
                    this-chunk
                    (make-point 6 2)
                    (tile 'stairs-down rotation))
                this-chunk)
            ((= height (+ roof-start 2))
                (define this-chunk (filled-chunk (tile 'air)))
                (define fence 'concrete-fence)
                (let
                    (
                        (locked-rotation-a (cond ((= rotation side-right) side-down) ((= rotation side-left) side-up) (else rotation)))
                        (locked-rotation-b (cond ((= rotation side-right) side-up) ((= rotation side-left) side-down) (else rotation))))
                    (begin
                        (big-fill-area this-chunk (make-point (make-point 1 2) (make-point 1 12)) (tile fence (side-combine locked-rotation-b side-up)))
                        (big-fill-area this-chunk (make-point (make-point 12 1) (make-point 2 1)) (tile fence (side-combine locked-rotation-a side-up)))
                        (big-fill-area this-chunk (make-point (make-point 2 1) (make-point 2 1)) (tile fence (side-combine locked-rotation-a side-up)))
                        (big-fill-area this-chunk (make-point (make-point 2 14) (make-point 12 1)) (tile fence (side-combine locked-rotation-a side-down)))
                        (big-fill-area this-chunk (make-point (make-point 14 2) (make-point 1 12)) (tile fence (side-combine locked-rotation-b side-down)))))
                (big-put-tile this-chunk (make-point 14 14) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 1 1) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 1 14) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 14 1) (tile 'concrete-fence))
                (big-fill-area this-chunk (make-point (make-point 4 1) (make-point 8 1)) wall-tile)
                (big-fill-area this-chunk (make-point (make-point 4 2) (make-point 1 3)) wall-tile)
                (big-fill-area this-chunk (make-point (make-point 11 2) (make-point 1 3)) wall-tile)
                (big-fill-area this-chunk (make-point (make-point 5 4) (make-point 4 1)) wall-tile)
                (big-put-tile this-chunk (make-point 7 2) (single-marker (list 'light (light-intensity 0.7) '(0.5 0.0 0.0))))
                (big-put-tile this-chunk (make-point 9 4) (single-marker (list 'door side-left 'metal 2))))
            ((= height (+ roof-start 3))
                (big-fill-area (filled-chunk (tile 'air)) (make-point (make-point 4 1) (make-point 8 4)) wall-tile))))
    ((= height 0)
        (put-floor (filled-chunk (tile 'concrete-path))))
    ((= (remainder height 2) 0)
        (define this-chunk (put-floor (filled-chunk (tile 'air))))
        (let ((x (if (= (remainder height 4) 0) 6 9)))
            (big-put-tile
                this-chunk
                (make-point x 2)
                (tile 'stairs-down rotation)))
        this-chunk)
    (else
        (define furnitures-seed
            (seed-with
                (seed-with
                    (let ((x (assq 'building-seed (chunk-tags-at middle-position))))
                        (if debug-mode (if (null? x) (begin (display "building-seed not found") (newline) 0) x) x))
                    height)
                2222))

        (define this-chunk (filled-chunk (tile 'air)))
        (define (decide-enemy type)
            (if (eq? type 'normal)
                (pick-weighted 'zob 'runner 0.25)
                'bigy))
        (define (try-put-furniture pos t)
            (big-combine-markers this-chunk pos t))
        (define (chair-list side)
            (list
                'furniture
                'wood_chair
                side
                '(0.0 0.25 0.0) '(-0.2 0.2 0.0) '(0.2 0.15 0.0) '(0.0 0.0 0.0)))
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
        (put-outer-walls this-chunk)
        (big-put-tile this-chunk (make-point 5 2) wall-tile)
        (big-put-tile this-chunk (make-point 10 2) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 5 4) (make-point 6 1)) wall-tile)
        (if (= height 1)
            (begin
                (big-put-tile this-chunk (make-point 7 1) (tile 'air))
                (big-put-tile this-chunk (make-point 8 1) (tile 'air))
                (big-put-tile this-chunk (make-point 7 1) (single-marker (list 'door side-left 'metal 2)))))
        (big-put-tile this-chunk (make-point (if (random-bool) 11 12) 16) (single-marker (list 'door side-right 'metal 1)))
        (let ((x (if (= (remainder height 4) 3) 6 9)))
            (big-put-tile
                this-chunk
                (make-point x 2)
                (tile 'stairs-up rotation)))
        this-chunk))

))

)
