(define (generate-chunk middle-position part)

(define building-height (assq 'building-height (chunk-tags-at middle-position)))

(if (>= height building-height) (filled-chunk (tile 'air))
(begin

(define big-size-x (* size-x 3))
(define big-size-y (* size-y 3))

(define in-big-chunk-pos
    (point-zip-map
        (make-point size-x size-y)
        (cond
            ((eq? part 'bl) (make-point 0 2))
            ((eq? part 'b) (make-point 1 2))
            ((eq? part 'br) (make-point 2 2))
            ((eq? part 'l) (make-point 0 1))
            ((eq? part 'm) (make-point 1 1))
            ((eq? part 'r) (make-point 2 1))
            ((eq? part 'tl) (make-point 0 0))
            ((eq? part 't) (make-point 1 0))
            (else (make-point 2 0)))
        (lambda (x y) (* x y))))

(define (big-put-tile this-chunk pos fill-tile)
    (let (
            (scaled-start (point-sub pos in-big-chunk-pos))
            (clip-check (lambda (v s) (and (> v -1) (< v s)))))
        (if (and
                (clip-check (point-x scaled-start) size-x)
                (clip-check (point-y scaled-start) size-y))
            (put-tile this-chunk scaled-start fill-tile)
            this-chunk)))

(define (big-fill-area this-chunk area fill-tile)
    (let ((scaled-start (point-sub (area-start area) in-big-chunk-pos)))
        (let (
                (scaled-end (point-zip-map (point-add scaled-start (area-size area)) (make-point size-x size-y) (lambda (x y) (min x y))))
                (clipped-start (point-map scaled-start (lambda (x) (max x 0)))))
            (let ((clipped-size (point-sub scaled-end clipped-start)))
                (if (and
                        (and (> (point-x scaled-end) 0) (< (point-x scaled-start) size-x))
                        (and (> (point-y scaled-end) 0) (< (point-y scaled-start) size-y)))
                    (fill-area this-chunk (make-area clipped-start clipped-size) fill-tile)
                    this-chunk)))))

(define wall-tile (tile 'concrete))

(define (put-outer-walls this-chunk)
    (big-fill-area
        (big-fill-area
            (big-fill-area
                (big-fill-area
                    (big-fill-area
                        (big-fill-area
                            this-chunk
                            (make-area (make-point 1 1) (make-point 7 1))
                            wall-tile)
                        (make-area (make-point 16 1) (make-point 7 1))
                        wall-tile)
                    (make-area (make-point 1 2) (make-point 1 21))
                    wall-tile)
                (make-area (make-point 22 2) (make-point 1 21))
                wall-tile)
            (make-area (make-point 2 22) (make-point 20 1))
            wall-tile)
        (make-area (make-point 7 0) (make-point 10 1))
        wall-tile))

(define (put-floor this-chunk)
    (big-fill-area
        (big-fill-area
            (put-outer-walls this-chunk)
            (make-area (make-point 2 2) (make-point (- big-size-x 4) (- big-size-y 4)))
            (tile 'wood))
        (make-area (make-point 8 1) (make-point size-x 4))
        (tile 'concrete)))

(define roof-start (- building-height 4))

(cond
    ((> height roof-start)
        (cond
            ((= height (+ roof-start 1)) (begin
                (define this-chunk
                    (big-fill-area
                        (big-fill-area
                            (filled-chunk (tile 'air))
                            (make-area (make-point 1 1) (make-point 22 22))
                            (tile 'concrete))
                        (make-area (make-point 7 0) (make-point 10 1))
                        (tile 'concrete)))
                (big-put-tile
                    this-chunk
                    (make-point 9 2)
                    (tile 'stairs-down rotation))
                this-chunk))
            ((= height (+ roof-start 2))
                (define this-chunk (filled-chunk (tile 'air)))
                (define fence 'concrete-fence)
                (let
                    (
                        (locked-rotation-a (cond ((= rotation side-right) side-down) ((= rotation side-left) side-up) (else rotation)))
                        (locked-rotation-b (cond ((= rotation side-right) side-up) ((= rotation side-left) side-down) (else rotation))))
                    (begin
                        (big-fill-area this-chunk (make-point (make-point 1 2) (make-point 1 20)) (tile fence (side-combine locked-rotation-b side-up)))
                        (big-fill-area this-chunk (make-point (make-point 17 1) (make-point 5 1)) (tile fence (side-combine locked-rotation-a side-up)))
                        (big-fill-area this-chunk (make-point (make-point 2 1) (make-point 5 1)) (tile fence (side-combine locked-rotation-a side-up)))
                        (big-fill-area this-chunk (make-point (make-point 2 22) (make-point 20 1)) (tile fence (side-combine locked-rotation-a side-down)))
                        (big-fill-area this-chunk (make-point (make-point 22 2) (make-point 1 20)) (tile fence (side-combine locked-rotation-b side-down)))))
                (big-put-tile this-chunk (make-point 22 22) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 1 1) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 1 22) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 22 1) (tile 'concrete-fence))
                (big-fill-area this-chunk (make-point (make-point 7 0) (make-point 10 1)) wall-tile)
                (big-fill-area this-chunk (make-point (make-point 7 1) (make-point 1 4)) wall-tile)
                (big-fill-area this-chunk (make-point (make-point 16 1) (make-point 1 4)) wall-tile)
                (big-fill-area this-chunk (make-point (make-point 8 4) (make-point 6 1)) wall-tile)
                (big-put-tile this-chunk (make-point 14 4) (single-marker (list 'door side-left 'metal 2))))
            ((= height (+ roof-start 3))
                (big-fill-area (filled-chunk (tile 'air)) (make-point (make-point 7 0) (make-point 10 5)) wall-tile))))
    ((= height 0)
        (put-floor (filled-chunk (tile 'concrete-path))))
    ((= (remainder height 2) 0) (begin
        (define this-chunk (put-floor (filled-chunk (tile 'air))))
        (let ((x (if (= (remainder height 4) 0) 9 14)))
            (big-put-tile
                this-chunk
                (make-point x 2)
                (tile 'stairs-down rotation)))
        this-chunk))
    (else (begin
        (define this-chunk (filled-chunk (tile 'air)))
        (put-outer-walls this-chunk)
        (big-fill-area this-chunk (make-area (make-point 7 2) (make-point 1 3)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 16 2) (make-point 1 3)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 2 17) (make-point 8 1)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 14 17) (make-point 8 1)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 9 4) (make-point 1 13)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 14 4) (make-point 1 13)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 10 16) (make-point 4 1)) wall-tile)
        (if (= height 1)
            (begin
                (big-put-tile this-chunk (make-point 11 0) (tile 'air))
                (big-put-tile this-chunk (make-point 12 0) (tile 'air))
                (big-put-tile this-chunk (make-point 11 0) (single-marker (list 'door side-left 'metal 2)))))
        (big-put-tile this-chunk (make-point 8 4) (tile 'concrete))
        (big-put-tile this-chunk (make-point 15 4) (tile 'concrete))
        (big-put-tile this-chunk (make-point 9 8) (single-marker (list 'door side-down 'metal 1)))
        (big-put-tile this-chunk (make-point 14 8) (single-marker (list 'door side-down 'metal 1)))
        (big-put-tile this-chunk (make-point (if (random-bool) 11 12) 16) (single-marker (list 'door side-right 'metal 1)))
        (let ((x (if (= (remainder height 4) 3) 9 14)))
            (big-put-tile
                this-chunk
                (make-point x 2)
                (tile 'stairs-up rotation)))
        this-chunk)))

))

)

;                        ((eq? op 'room-seed)
;                            (lambda (chunk-position)
;                                (lambda (room-number)
;                                    (random-integer-seeded
;                                        (wrapping-add
;                                            (random-integer-seeded
;                                                (wrapping-add
;                                                    (assq 'building-seed (chunk-tags-at chunk-position))
;                                                    height))
;                                            room-number)))))
