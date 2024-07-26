(define size-x 16)
(define size-y 16)

(define (filled-chunk tile)
    (make-vector (* size-x size-y) tile))

(define (index-of point)
    (+ (* size-x (point-y point)) (point-x point))) 

(define make-point cons)
(define point-x car)
(define point-y cdr)

(define side-up 0)
(define side-down 1)
(define side-left 2)
(define side-right 3)

; advanced rng, why do i even have this?
(define random-side side-up)

(define (put-tile chunk pos tile)
    (vector-set!
        chunk
        (index-of pos)
        tile)
    chunk)

(define (vertical-line-length chunk pos len tile)
    (if (= len 0)
        chunk
        (let ((x (point-x pos)) (y (point-y pos)))
            (vertical-line-length
                (put-tile
                    chunk
                    (make-point x (+ y (- len 1)))
                    tile)
                pos
                (- len 1)
                tile))))

(define (vertical-line chunk x tile)
    (vertical-line-length chunk (make-point x 0) size-y tile))

(define (horizontal-line-length chunk pos len tile)
    (if (= len 0)
        chunk
        (let ((x (point-x pos)) (y (point-y pos)))
            (horizontal-line-length
                (put-tile
                    chunk
                    (make-point (+ x (- len 1)) y)
                    tile)
                pos
                (- len 1)
                tile))))

(define (horizontal-line chunk y tile)
    (horizontal-line-length chunk (make-point 0 y) size-x tile))

(define (fill-area chunk pos size tile)
    (if (= (point-x size) 0)
        chunk
        (begin
            (vertical-line-length
                chunk
                (make-point (- (+ (point-x pos) (point-x size)) 1) (point-y pos))
                (point-y size)
                tile)
            (fill-area
                chunk
                pos
                (make-point (- (point-x size) 1) (point-y size))
                tile))))
